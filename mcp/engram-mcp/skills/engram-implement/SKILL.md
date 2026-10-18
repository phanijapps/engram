---
name: engram-implement
description: Execute a plan step-by-step with a self-review loop — implement, verify with the plan's own commands, review the diff against the plan's risks, then learn from the outcome. Trigger when a plan exists (from engram-plan or given directly) and it's time to execute.
---

# engram-implement

Implement with discipline: **implement → verify → self-review**, per step.
Red tests are information, not failure. Flailing is forbidden.

## Contract

1. Work the plan's steps in order; verify each with its own command.
2. Self-review every diff against the plan's Findings and Risks.
3. Two consecutive failed verifications on one step → stop and report.
4. The session ends with a learn pass (`engram-learn` core ideas).

## Workflow

### 0. Recall the plan

If the plan isn't in-context:

```
recall({ "query": "PLAN <slug>" })
```

No plan anywhere? Either draft one (engram-plan, even briefly) or state
plainly that this is plan-less execution and accept the label.

### 1. Per step: implement

Make the change. Small diffs. Prefer the smallest edit that satisfies the
step — style sweeps and drive-by refactors belong in their own step
(usually: don't).

### 2. Per step: verify

Run the step's verification command exactly as written in the plan. If
the plan's command was wrong, fix the plan's command (and note it —
that's a learning).

- **Green** → self-review (step 3).
- **Red** → read the failure; fix; re-verify. The red IS the signal.

### 3. Per step: self-review

Before moving on, review your diff against the plan:

- Does the diff do what the step said — nothing more?
- Did it touch any Risk-listed symbol/area? If yes, is the mitigation applied?
- Did it break an invariant the plan's Findings flagged?
- New behavior covered by a test (or is one honestly not warranted)?

Any "no" → fix before proceeding. Skipping self-review to "come back
later" is how bridges get broken silently.

### 4. Stop conditions

- All steps verified + self-reviewed → completion report → step 5.
- Two consecutive failed verifications on the SAME step → STOP. Report:
  the step, what you tried (both attempts), the error, your read on the
  blocker. Do not attempt three.
- Plan invalidated mid-flight (a finding proves wrong) → stop, report the
  contradiction, propose the plan amendment. Don't improvise a new plan
  silently.

### 5. Completion + learn pass

Report: steps done, verification results, diff summary, plan amendments
made, risks re-checked. Then a quick learn pass — the durable bits only:

```
write_memory({ "content": "Fixed <X>: root cause was <Y>. The plan's
  step-N command was wrong (<what worked instead>)." })
procedure_put({ "name": "<validated workflow, if one emerged>",
  "trigger": "<when>", "steps": ["..."] })
```

## When it fails

- Verify command can't run (missing tool/deps) → report immediately; do
  not substitute a weaker check silently.
- Scope creep detected mid-step → finish or revert the current step, then
  raise the scope change explicitly.

## Anti-patterns

- Implementing multiple steps before verifying any.
- "I'll add tests at the end."
- Third attempt after two failures (stop means stop).
- Editing the plan to match the code after the fact without saying so.

## Final check

- Every step verified green, every diff self-reviewed?
- Risks from the plan re-checked against the final diff?
- Did the learn pass capture the root causes, not the play-by-play?
