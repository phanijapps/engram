## Full Team
| Stage | Agent | Writes or edits |
| --- | --- | --- |
| Analyze | engram-analyzer | analysis.md |
| Plan | engram-planner | plan.md |
| Tasks | engram-tasks | tasks.md, verification.md, release.md |
| Build | engram-builder | Source, tests, configuration exports, task progress, build.md |
| Review | engram-code-reviewer<br>engram-security-reviewer<br>engram-platform-reviewer<br>engram-ux-reviewer<br>engram-ui-reviewer<br>engram-backend-reviewer<br>engram-data-reviewer<br>engram-operations-reviewer | review-code.md<br>review-security.md<br>review-platform.md<br>review-ux.md<br>review-ui.md<br>review-backend.md<br>review-data.md<br>review-operations.md |
| Approve | engram-approver | decision.md |


## Light Mode
| Stage | Agent | Writes or edits |
| --- | --- | --- |
| Analyze | engram-analyzer | analysis.md |
| Plan | engram-planner | plan.md |
| Tasks | engram-tasks | tasks.md, verification.md, release.md |
| Build | engram-builder | Source, tests, configuration exports, task progress, build.md |
| Review | red-forman | review-code.md<br>review-security.md<br>review-platform.md<br>review-ux.md<br>review-ui.md<br>review-backend.md<br>review-data.md<br>review-operations.md |
| Approve | engram-approver | decision.md |

## Mode selection

`create --mode <full-or-light>` is the owner pin; absent mode means `full`.
The analyzer's frontmatter `mode` recommendation is applied at analyze
completion only when no pin exists — a differing declaration on a pinned
job is refused and recorded as a `mode-refused` history event. A human
rework note supersedes any pin and starts a new cycle. The board job record
carries the mode and `status` shows it.