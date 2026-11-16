# Agent contract directory

Every generated wrapper reads references/shared.md and exactly one role contract.
Shared rules are authored once; the files below contain only role responsibilities.

| Role | Contract |
| --- | --- |
| engram-analyzer | references/roles/analyzer.md |
| engram-planner | references/roles/planner.md |
| engram-tasks | references/roles/tasks.md |
| engram-builder | references/roles/builder.md |
| engram-qa | references/roles/qa.md |
| engram-approver | references/roles/approver.md |
| engram-*-reviewer | references/roles/reviewer.md plus assigned discipline |

Review lenses/rubrics live once in references/reviews.md. Phase-specific state and
correction protocols live in their phase references, not duplicated wrapper bodies.
Agent names, descriptions, discipline and host permissions are authored in
agents/roles.json. scripts/generate_agents.py generates four dialects and Pi
mirrors. See references/agent-maintenance.md for generation/check/install updates.
