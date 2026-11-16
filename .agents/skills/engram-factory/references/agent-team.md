# Agent team

| Phase | Agent | Output |
| --- | --- | --- |
| Spec | engram-analyzer | compact spec.md |
| Plan | engram-planner | compact plan.md and declared plain supports |
| Tasks | engram-tasks | tasks/task-NNN.md, candidate manifest, release.md |
| Build | engram-builder | assigned source work, task evidence, round build handoff |
| Optional E2E | engram-qa | candidate-bound scenario report and raw evidence |
| Independent review | applicable specialist agents | separate per-discipline reports |
| Approval recommendation | engram-approver | reviews/final-review.md |

Framing is orchestrator-run with the owner. The orchestrator owns controller
state transitions and exact-plan Start/Finish gates. Full mode uses separate
applicable specialist invocations; light mode uses a separate red-forman
invocation to author every required discipline report under the same rubric.
Both modes preserve source/evidence binding, required verdicts and owner authority.
