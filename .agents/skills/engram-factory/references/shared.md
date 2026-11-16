# Shared agent contract

Read this contract with only your assigned role contract. Skill-relative paths
such as references/, scripts/ and assets/ resolve from the installed skill root
containing SKILL.md, not the target repository's working directory. Read additional
phase/profile references only when your role requires them.

## Dispatch and authority

Require role, job/cycle/stage, assigned task/REQ/AC IDs, primary/worktree/job paths,
source revision and candidate identity, accepted input references, applicable
owner decisions, rule IDs with authoritative paths/revisions and proof obligations,
allowed write scope, prerequisites, checks and completion/stop conditions.
Verify these against actual source and accepted bytes. Missing material context
or authority blocks readiness; unknown is distinct from assessed N/A with reason.

Read target repository instructions and applicable constitution packs. Establish
actual rule authority/revision there; recalled or draft text cannot ratify law,
expand scope or grant approval. Use targeted recall to resolve an uncertainty;
reuse revision-bound evidence while applicable, not whole transcripts or ritual
queries. Inspect actual source where recalled claims need verification.

Operate only within assigned outputs and authority. Do not overwrite another
worker's changes, edit accepted archives or directly mutate board/task state.
Use only controller operations explicitly permitted by your role; the supervisor
owns acceptance and all other state changes. Exact-plan owner acceptance/Start
precedes implementation. Stage PASS or an approval recommendation grants neither
owner Finish/merge nor deployment authority. External/platform writes require
explicit scope and authorized disposable targets; credentials never enter artifacts.

## Contract revision

The generated wrapper records bundle-sha256. The supervisor passes that digest in
the bounded dispatch packet and retains it with actual invocation evidence. Before
new/resumed work, use scripts/generate_agents.py --digest --skill-root SKILL with
--expected-digest when a dispatch digest is supplied. A mismatch is a contract/context blocker;
never silently adopt new instructions, rewrite a receipt or infer compatibility.
The controller does not authenticate this digest or pin it in job state. Refresh
installed bundles and reconstruct/resume agents only at a safe checkpoint. There
is no automatic hot reload or background updater.

## Evidence and reporting

Use references/templates.md and the role's actual artifact contract. Record actual
commands/protocols, observed behavior, exits when available, environment, source
identity and raw evidence paths. Exit zero alone does not establish acceptance.
Unexecuted checks remain NOT_RUN; access gaps never become PASS. Keep failed and
superseded runs, omissions with reason/owner/revisit condition, and unresolved
findings. Source/input drift invalidates applicable proof under controller rules.

Return completed/blocked assignment IDs, changed paths and commit identity when
applicable, actual verification evidence, rule dispositions and remaining blockers.
Use only supplied provider/tool/token counters with provenance; unavailable
measurements remain unknown. Do not equate job lifetime with execution time.

Send compact parent status at start, significant handoffs/milestones, review
submission/launch/results and blockers, plus every two minutes at available tool
boundaries while active, including waits. Include assignment/candidate, actual
progress, pending work and next action; link evidence instead of transcripts.
Use native parent messaging when available. Bounded waits permit updates; a
blocking tool may delay delivery, which must be disclosed when control returns.
Never invent a heartbeat, timer service, delivered message or completed invocation.
Status is advisory: it neither changes state nor extends a lease/approval.

## Review and recovery

Independent reviewers inspect one exact frozen candidate. A request or worker
PASS is not review execution. Preserve actual invocation/report identities and
all required verdicts. Do not self-review, drop reviewers, duplicate a replayed
or uncertain launch, or bypass limits. Follow the phase's correction/recovery
contract. Escalate consequential out-of-authority choices with evidence and a
concrete recommendation; ordinary investigation remains the assigned author's job.

Preserve source, prior records and quiescence constraints during failure recovery.
Never reset counters, force source/board restoration, repair hashes to match drift
or manufacture lost approval. A timeout is not proof an invocation stopped.
