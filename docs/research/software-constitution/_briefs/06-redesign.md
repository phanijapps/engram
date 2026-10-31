# Brief: Redesign architect (gpt-5.6-terra) — senior-grade templates that keep the gates

ROLE: Redesign engram's four SDD templates so a staff engineer would be proud to
write them — NARRATIVE-FIRST bodies, structure only where it earns its keep —
while preserving every machine contract (the owner will not accept losing the
enforcement layer that was just built).

## Hard constraints (non-negotiable, verified in code)
1. Frontmatter stays JSON-per-line (controller parser: one JSON value per line),
   file starts with `---`, and carries the union keys the controller validates:
   schema_version: 2, artifact/job_id/cycle/status/author_role/profiles/
   source_snapshot/input_artifacts/updated_at + size_tier (+ brief_ref etc.).
   Render placeholders {{job_id}} {{cycle}} {{profiles_json}} must survive.
2. Traceability survives: REQ/STEP/TASK/AC/GATE IDs and the REQ×task coverage
   matrix stay (check_job_artifacts.py validates them), but IDs live INSIDE
   prose where possible, not in giant placeholder tables.
3. EARS for requirements; BCP 14 keywords where normative.
4. S/M/L tiering survives (S = minimal sections, target ≤60 lines).
5. Verification evidence rows (gate → result) stay tabular — they ARE data.

## Inputs (read; cwd = /home/videogamer/projects/mem-alpha)
- Current templates: .agents/skills/engram-factory/assets/templates/{brief,spec,plan,tasks}.md
- Real v2 job: .engram-factory/jobs/011-en004-fail-closed/*.md
- Old rich prose style for flavor: .engram-factory/jobs/002-expose-agent-skills/analysis.md
- Gate: tools/ci/check_job_artifacts.py (know what it regexes)

## Deliver (to docs/research/software-constitution/06-redesigned-templates.md)
Four COMPLETE rewritten templates (brief/spec/plan/tasks) where:
- The body is the engineer's argument: opening paragraph that states the problem
  and the bet; prose sections; tables ONLY for true enumerations (requirements,
  gates, coverage) pre-filled with one REALISTIC worked example row each —
  a filled example teaches better than a blank placeholder
- Every section answers a question a reviewer actually asks
- An S-tier job filled from them would read like a strong design-doc email,
  not a form; L-tier scales by ADDING narrative, not more scaffolding
- Frontmatter block exactly as above (JSON-per-line, quoted strings)
Include a short "what changed and why" preamble. Under ~450 lines. PROPOSAL ONLY —
do NOT modify the live templates.
