# Task-author-led independent review

The task author owns drafting, structural correction, independent reviewer
launch/wait and finding resolution. The supervisor retains controller writes.
This uses existing task operations; there are no tasks-session/context/check/submit
commands and no implied spec/plan-style shared session implementation.

## Loop

1. Read the accepted brief, spec, plan and declared supports at their recorded
   revisions. Author task documents, a candidate manifest under
   records/operations/, and ready release metadata. Preserve accepted requirements
   and plan scope.
   Map each AC to behavior delivery plus verification; mappings may be many-to-many.
   Specify dependencies, bounded output/ownership, RED/GREEN proof and evidence
   destinations. Tests remain NOT_RUN during generation.
2. Return candidate paths to the supervisor for tasks-initialize. Before
   initialization, report structural preflight as pending, never PASS. Once
   initialized, run read-only `preflight --job ID --stage tasks` locally and fix
   reported document problems. Never edit tasks/manifest.json or tasks/task.json.
   If document hashes or manifest structure need revision, send the revised
   candidate to the supervisor for the appropriate existing controller operation.
   Use tasks-reinitialize only after recorded CHANGES under this workflow.
   The controller enforces fully-pending/no-evidence state but does not itself
   enforce the CHANGES receipt condition. This policy is not a generic repair
   path or permission to reset review history.
3. The supervisor freezes each task phase using `review-freeze --stage tasks
   --phase PHASE-N`, with the actual task author and independent roster. It returns
   the candidate number, phase, frozen input references and reviewer identities.
   This handoff is a required controller checkpoint, not an author-side submit API.
4. For each fresh frozen phase candidate, the task author spawns the separate
   required reviewers through native nested delegation and waits for actual
   reports. Review the entire phase candidate, including upstream contracts,
   manifest and release inputs selected by the controller. The author must not
   choose/drop reviewers or edit pending inputs. Use supervisor dispatch only
   when nested delegation is unavailable. Coordinate launch ownership with the
   supervisor by job/cycle/phase/candidate and retain actual invocation evidence;
   on replay or uncertain launch state inspect evidence rather than launching again.
5. Relay actual report references for supervisor review-record ingestion. For
   CHANGES, inspect all findings, fix task documents and the candidate manifest
   as needed, and return revised files for guarded reinitialization and freeze.
   That operation archives and supersedes all phase candidates because the
   manifest is shared: every phase needs fresh independent review. Do not reuse
   earlier PASS evidence against the revised manifest. Preserve superseded reports.
6. After all current phase candidates PASS and current inputs still match, return
   a compact result to the supervisor. Only the supervisor accepts tasks. Owner
   acceptance of the exact plan and Start still precede execution; Finish/merge
   remains separate. No task execution, complete, initialization, review-record,
   Start, rework, approve or merge powers are granted to the task author.

Follow shared.md for parent progress updates throughout: milestones/blockers plus
every two minutes at available tool boundaries, including review waits. Keep
messages compact and evidence-linked. Read-only status and persisted reports
are evidence; parent progress messages alone never certify review completion.
Use existing task-phase review/recovery rules; do not import spec/plan's three
candidate cap or invent an automatic unbounded retry. Escalate repeated unchanged
findings, review ERROR, missing authority or unavailable evidence to the supervisor.

For E2E-selected plans, manifest tasks remain implementation/test-authoring
work with focused proof. Reference the plan's separately executed E2E scenarios
in task handoffs; never block build completion on a deferred E2E campaign or
falsely complete an unexecuted E2E task. See e2e-phase.md.

Only Proof cells explicitly naming `E2E: E2E-NNN` are deferred phase execution.
Task authors reference those scenarios rather than copying them into build
completion commands. All other required task proof stays in the build phase.
