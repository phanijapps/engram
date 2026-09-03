# Claude Skills

Skills are workflows that Claude (and other agents) invoke for repeating
multi-step tasks. Each lives in `.claude/skills/<name>/SKILL.md` and is
auto-discovered by Claude Code.

## Skill trees and canonical sources

This repo carries three harness skill trees. To prevent drift, each skill
has exactly ONE canonical home; every other copy is a generated mirror:

| Tree | Harness | Role |
| ---- | ------- | ---- |
| `.claude/skills/` | Claude Code | **Canonical** for the generic workflow skills (with `*.upstream.*` companions from the agentbundle install) and the workspace-queue trio |
| `.codex/skills/` | Codex CLI | Mirror of the canonical `.claude/skills/` copies (`*.upstream.*` companions excluded) |
| `.agents/skills/` | pi (this repo's primary harness) | Canonical for the engram ops skills (`engram-live-verify`, `engram-perf-audit`, `engram-self-index`, `engram-store-ops`) and the `architect-*` pack |

Two more locations own skills that must never be copied into the trees
above — link or reference instead:

- `mcp/engram-mcp/extensions/` — canonical home of the MCP-facing skills
  (`engram-index`, `engram-find`, `engram-distill`). The entries in this
  directory are **symlinks** to that home.
- `~/.agents/skills/` — user-level packs (research, discovery), not
  repo-owned.

Sync rule: when a canonical skill changes, re-mirror it (`.claude` →
`.codex`, `*.upstream.*` excluded) in the same commit. The superseded
engram-specific workflow skills (`engram-contract`, `engram-plan`,
`engram-eval`, `engram-code-docs`) were removed 2026-09-03 — the generic
workflow skills plus the `.agents/skills/` ops skills cover the same
ground (see AGENTS.md § Local Codex Assets).

## When to add a skill

Add a skill when you've performed the same multi-step workflow **three times**.
Don't add one speculatively — speculative skills bloat context and degrade
adherence. The full four principles for what we keep — universal across
tech stacks, substantive rather than duplicative, a habit rather than a
tool, used often enough to stick — live in
[`docs/CHARTER.md`](../../docs/CHARTER.md) § Principles.

## Skills in this repo

| Skill | What it does |
| ----- | ------------ |
| [`work-loop`](work-loop/SKILL.md) | The standard plan → execute → verify → review loop for non-trivial work. Start here for any feature, fix, or refactor. |
| [`new-adr`](new-adr/SKILL.md) | Create a new ADR with the next available number, from the template |
| [`new-rfc`](new-rfc/SKILL.md) | Open a new RFC with a research-phase gate (repo + external sweep, recommendations on unresolved questions) before drafting |
| [`new-spec`](new-spec/SKILL.md) | Scaffold a new spec directory, surface assumptions, then fill `spec.md` and `plan.md` |
| [`bug-fix`](bug-fix/SKILL.md) | Fix a defect — reproduce → failing test → root cause → minimum fix → root-vs-symptom verify → commit body documents *why* |
| [`update-conventions`](update-conventions/SKILL.md) | Open an RFC to change `docs/CONVENTIONS.md` |
| [`adapt-to-project`](adapt-to-project/SKILL.md) | Walk the adopter through the four classes of post-install change (substitution, `.upstream` companion merges, discovery + restructuring, within-layout consolidation). Per-scope; class-1 shells out to `agentbundle adapt` |
| [`new-guide`](new-guide/SKILL.md) | Draft a new user-facing guide under `docs/guides/<quadrant>/` following the Diátaxis framework |
| [`init-project`](init-project/SKILL.md) | Initialize the workspace scaffolding for a fresh adopter |
| [`contract-acquisition`](contract-acquisition/SKILL.md) | Gather the contracts a pack needs from the adopting repo |
| [`receive-brief`](receive-brief/SKILL.md) | Decompose a DoR-compliant brief into specs and queue them |
| [`author-brief`](author-brief/SKILL.md) | Turn unstructured external input into a DoR-compliant brief queued in `workspace.toml` |
| [`capture-work`](capture-work/SKILL.md) | Capture a session's surfaced future-work list into `workspace.toml` |
| [`workspace-status`](workspace-status/SKILL.md) | Orient at session start — surface ready/blocked/parallel items from `workspace.toml` |
| [`loop-da-loop`](loop-da-loop/SKILL.md) | Lightweight iterate-until-done loop for small, well-defined change sets |
| [`operational-safety`](operational-safety/SKILL.md) | Operational-safety gates for infrastructure-touching work |
| [`security-checklists`](security-checklists/SKILL.md) | Security review checklists keyed to the change surface |
| [`engram-index`](engram-index/SKILL.md) → `mcp/engram-mcp/extensions/` | Index a codebase and pull the structural overview via the engram MCP |
| [`engram-find`](engram-find/SKILL.md) → `mcp/engram-mcp/extensions/` | Onboard onto a codebase via the engram MCP |
| [`engram-distill`](engram-distill/SKILL.md) → `mcp/engram-mcp/extensions/` | Distill docs/transcripts into the knowledge graph via the engram MCP |

## Authoring skills

Each `SKILL.md` should:

1. Have a tight YAML frontmatter `description` — the *trigger surface*
   that decides invocation. The body answers the disjoint question of
   what to do once invoked (preconditions, judgment, procedure); it must
   not restate the trigger.
2. Be small. Skills are loaded into context when triggered; bloated skills
   crowd out the user's actual task.
3. Link out to scripts in the same directory rather than embedding shell.
   Code in scripts can be tested; code in markdown can't.
4. Refer to other skills, subagents, and conventions sections by name
   in the body — the same way `work-loop` cites `new-spec`,
   `adversarial-reviewer`, and `docs/CONVENTIONS.md`. The body is the
   contract; resolution happens at runtime against whatever the
   adopter has installed.
