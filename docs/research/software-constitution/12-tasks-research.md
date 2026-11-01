# Tasks format research — v5

## Method and sources

Fetched 2026-11-01; quoted formats below are from the retrieved current files.

1. [spec-kit tasks template](https://raw.githubusercontent.com/github/spec-kit/main/templates/tasks-template.md)
   and its [plan template](https://raw.githubusercontent.com/github/spec-kit/main/templates/plan-template.md).
2. [OpenSpec spec-driven tasks template](https://raw.githubusercontent.com/Fission-AI/OpenSpec/main/schemas/spec-driven/templates/tasks.md)
   and [a current change task list](https://raw.githubusercontent.com/Fission-AI/OpenSpec/main/openspec/changes/simplify-skill-installation/tasks.md).
3. [Kiro Specs documentation](https://kiro.dev/docs/specs/) and Kiro's
   [first-party issue-automation tasks](https://raw.githubusercontent.com/kirodotdev/Kiro/main/.kiro/specs/github-issue-automation/tasks.md).

## What the references actually do

### spec-kit

- Defines `- [ ] T001 ...`; its declared format is `[ID] [P?] [Story] Description`.
- `[P]` means different files and no dependencies; descriptions include exact paths.
- Groups checklist items into setup, foundation, story, and polish phases. Tests are
  optional, but when present are explicit tasks before implementation.
- Its plan separates technical context, constitution gate, structure, and complexity
  tracking; it does not prescribe a task-result table.

### OpenSpec

- The shipped template is only `## 1. <group>` followed by `- [ ] 1.1 <task>` and
  further numbered groups; it has no task IDs, tables, verification fields, or prose.
- The fetched live change uses that exact pattern: numbered group headings and completed
  `- [x] 1.1 ...` implementation lines, including test tasks in their relevant group.
- This is the strongest evidence for a compact hierarchical numbered checklist.

### Kiro

- Docs describe `tasks.md` as discrete, trackable executable work with real-time task
  status; its runner derives a dependency graph and runs independent tasks concurrently
  in waves.
- Kiro's first-party example uses `- [ ] 2. Implement ...` with nested `- [ ] 2.1
  Create ...`, imperative labels, requirement annotations, and test/checkpoint tasks.
- The fetched example marks optional work `[ ]*`; it does **not** use a literal `[P]`
  marker or a literal per-task `Verify:` line. The v5 marker is therefore a local,
  explicit complement to Kiro's documented automatic parallelism, not a claimed Kiro
  syntax; the gate line makes its requirement/checkpoint trace executable.

## v5 decisions

- **Take:** OpenSpec's numbered group/subtask checklist and checkbox completion state.
- **Take:** spec-kit's `[P]`, imperative path-bearing actions, and explicit test work.
- **Take:** Kiro's granular requirement-linked task/checkpoint style; adapt it to one
  gate-backed `Verify:` line because this repository needs runnable evidence anchors.
- **Take:** inline TASK/REQ/AC/STEP/GATE/path tags so traceability remains visible
  without recreating the v4 eight-column table.
- **Reject:** spec-kit's phase essays, project-layout boilerplate, and story strategy;
  jobs already have a plan and spec.
- **Reject:** OpenSpec's absence of trace and verification fields; it cannot carry the
  factory contract alone.
- **Reject:** Kiro's lengthy overview/notes and optional `*` convention; neither is
  needed for the lean task artifact.

## Local evidence

- Index result: index empty for `software constitution tasks template checklist verification`.
- Index result: index empty for `job 011 fail closed engine neutrality GATE-001 tasks`.
- Worked-example IDs and gates were read from `.engram-factory/jobs/011-en004-fail-closed/`
  `spec.md` and `plan.md`; `GATE-001` is the two-case fixture and `GATE-002` is the
  clean engine-neutrality command.
