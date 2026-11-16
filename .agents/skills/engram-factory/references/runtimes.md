# Runtime adapters

Treat names as logical roles, not proof native agents exist. Inspect actual host
capabilities and local instructions. Never guess agent APIs, model IDs, native
configuration formats or resume features.

| Capability | Behavior |
| --- | --- |
| Separate invocation | Required for substantive roles and independent reviews; otherwise block |
| Parallel invocation | Preferred for reviews; serial independent invocations acceptable |
| Agent resume | Reuse builder when available; otherwise reconstruct persisted context |
| Tool restrictions | Apply per-role limits where supported; prompts are not a sandbox |
| Local execution | Bundled controller requires Python 3.10+, Git worktrees and POSIX locks |
| Platform connectivity | Required for any claimed environment verification, optional for planning |

Use native delegation for Claude, Codex, Pi or another host only when available.
references/shared.md and references/roles/ contain the canonical instructions. The installer delivers the factory
specialists in the four documented dialects (claude markdown, codex YAML,
copilot markdown, pi markdown) from the skill's own `agents/` payload.
For Pi, `.pi/agents/engram-*.md` (analyzer, planner, tasks, builder, the eight
discipline reviewers, approver, and engram-qa for executable acceptance
evidence) are installer-regenerated delivered artifacts of the pi dialect;
each wrapper points to its shared/role contracts, and the parity
check fails on byte drift. Intent framing is orchestrator-run via the
engram-frame skill — no child agent holds the owner channel. Other hosts:
generate native agent files in prescribed directories only after reading the
runtime's current official specification and repository instructions.
This package installs no formats beyond the four documented dialects.

Pass minimum evidence instead of entire conversation history. Keep builder,
reviewers and approver distinct. The controller manages local state, not remote
agent sessions. It does not provide SSO, sandboxing, CI attestations, centralized
audit retention, live drift detection, deployment execution or a dashboard server.
Connect organization controls separately; board.json is the dashboard input.

## Shared spec request consumption

Spec sessions expose controller context/check/submission, not a remote-agent
scheduler. Runtime delegation may consume a fresh immutable request once against
its exact candidate and fixed roster. Verify current input applicability before
launch, deduplicate by request identity, and retain actual invocation evidence.
On replay/unknown launch state inspect persisted evidence instead of launching
again. If host delegation is unavailable, return needs-dispatch to the supervisor;
never self-review or infer completion. See spec-session.md for crash boundaries.

Plan requests use the same native-dispatch contract as spec requests, with
phase=plan and an exact plan/support draft fingerprint. Dispatch only the named
phase/candidate/roster, never infer a process launched from request creation.

For spec/plan sessions the analyzer/planner initiates native nested reviewer
delegation and owns the wait/correction loop when supported. Supervisor launch
is the fallback. Authors relay actual report references for supervisor ingestion;
controller evidence and supervisor acceptance remain separate from status messages.

Task authors use the same nested reviewer delegation/status-update pattern on
supervisor-frozen per-phase candidates. Initialization, freeze, reinitialization,
report ingestion and acceptance remain supervisor operations; see tasks-review.md.

Generated wrappers and installation refresh follow references/agent-maintenance.md.
Runtime checks use canonical-only digest mode; repository checks also verify
generated files and Pi mirrors. No runtime reload or remote updater is implied.
