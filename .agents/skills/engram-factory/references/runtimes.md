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
references/agents.md is canonical. Generate native agent files in prescribed
directories only after reading the runtime's current official specification and
repository instructions. This package does not install guessed native configs.

Pass minimum evidence instead of entire conversation history. Keep builder,
reviewers and approver distinct. The controller manages local state, not remote
agent sessions. It does not provide SSO, sandboxing, CI attestations, centralized
audit retention, live drift detection, deployment execution or a dashboard server.
Connect organization controls separately; board.json is the dashboard input.
