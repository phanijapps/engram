# Brief: Rebuild tasks template from OpenSpec + spec-kit + Kiro (gpt-5.6-terra)

The current tasks proposal is over-engineered (8-column table, lifecycle
essays). Real tools keep tasks SIMPLE. Rebuild it grounded in the three
references. You write ONLY docs/research/software-constitution/
proposed-templates/tasks.md (replace content) and a short research note
docs/research/software-constitution/12-tasks-research.md.

## Fetch and extract the REAL formats
1. github/spec-kit tasks template:
   https://raw.githubusercontent.com/github/spec-kit/main/templates/tasks-template.md
   (also plan-template.md for continuity)
2. OpenSpec tasks.md format — from the repo (Fission-AI/OpenSpec): find the
   change-template tasks.md (try
   https://raw.githubusercontent.com/Fission-AI/OpenSpec/main/src/templates/change/tasks.md
   and adjacent paths; or the README's example). Numbered hierarchical items.
3. AWS Kiro spec format — kiro.dev docs on specs/tasks (search
   site:kiro.dev or the Kiro GitHub examples): numbered 1.1/1.2 checkboxes,
   [P] parallel markers, imperative action labels, per-task verification.

## Design the engram tasks v5 (LEAN)
- Core = a numbered hierarchical checklist (spec-kit/Kiro/OpenSpec style):
  `1.`, `1.1`, checkboxes `[ ]`/`[x]`, imperative one-line actions, optional
  `[P]` parallel marker, inline `(REQ-001, AC-001, GATE-001, STEP-001, path)`
  tag on each top-level item — IDs INLINE, not table columns.
- Per-item verification one-liner (Kiro style) where a gate proves it.
- ONE short "rules" block (5-7 lines max): checkbox semantics (PASS or
  linked exception only), stop condition one line, verification.md is the
  sole result record (link anchors), coverage = every REQ/AC appears in some
  item's tag (until the gate derives it) — one example line, no essays.
- Frontmatter: EXACTLY the union contract (copy from the current file's
  frontmatter — JSON-per-line, {{render}} placeholders, schema_version 2,
  artifact "tasks", plan_ref etc.). Comment header: integration note + v5
  provenance (OpenSpec numbered deltas, spec-kit numbered checklist + test
  column spirit, Kiro [P] + per-task verification).
- Total under 55 lines INCLUDING frontmatter. Worked example: job 011
  fail-closed gate (two items + one fixture sub-item).
12-tasks-research.md: what each reference actually does (with fetched
evidence), what you took and rejected, under 80 lines.
