# Brief: Tasks-template review — builder-workflow lens (gpt-6.1-sol)

ROLE: Review engram's proposed tasks template as the BUILDER who executes from
it daily. Verdict + concrete fixes. Review only — write ONE file:
docs/research/software-constitution/10-tasks-review-builder.md

## Read (cwd = /home/videogamer/projects/mem-alpha)
- docs/research/software-constitution/proposed-templates/tasks.md (the proposal)
- .engram-factory/jobs/011-en004-fail-closed/tasks.md (real v2 output)
- .agents/skills/engram-factory/assets/templates/tasks.md (live template)
- docs/research/software-constitution/05-spec-craft-analysis.md (§tasks principles)
- docs/research/software-constitution/07-comparative-verdict.md (merge criteria)

## Review questions (answer each with line evidence, 1-5 score, fix)
1. Executability: could a builder start work from this alone — is every TASK
   row independently verifiable, is the checkbox flow honest (NOT_RUN vs
   PASS/FAIL), are stop conditions actionable rather than bureaucratic?
2. Coverage matrix: does the REQ×task table earn its existence, or is it
   derivable from the TASK rows (critic criterion 4 allows compact mapping)?
   Recommend keep-table / kill-table / derive-in-gate.
3. Test-scenarios-as-code doctrine (TASK rows bound to AC+gate+fixture path):
   is it clearly taught? Does the worked example (TASK-002) show the pattern?
4. Delivery concerns + review findings: do these sections earn their place at
   S-tier, or should they be mandatory-when-applicable one-liners?
5. Friction: what still reads as form-filling? What would you DELETE?
6. Missing: anything a builder needs that is absent (execution log? commit
   granularity? blocked-state recording? handoff to verification.md)?
Output: verdict (ship / fix-then-ship / rethink), scored findings, and a
short PATCH LIST of exact edits (old -> new). Under 150 lines, date 2026-11-01.
