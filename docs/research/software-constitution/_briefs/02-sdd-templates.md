# Task brief: SDD template redesign (perspective: information-architecture / spec quality)

ROLE: Redesign engram's Spec-Driven-Development artifact templates. Owner's complaint:
specs/plans/tasks "follow a template but are not perfect — some jobs are noisy and slow".
Diagnose WHY from real evidence, then propose better templates that a Software Constitution
can gate at CI time. You answer the owner's **Q2–Q5** (brief/intent, spec, plan, tasks).

## Standard sources (fetch the principles page at minimum)
- https://softwareconstitution.com/principles
- https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md
- Philosophy: "wiki pages don't enforce design principles — gates do." Rules must be
  machine-checkable; six rule classes incl. RULE-3 zero-stubs and RULE-4 canonical-first
  evidence events; DEVIATIONS.md time-boxed exceptions; gates fail CI at merge time.

## Study the current system (cwd = /home/videogamer/projects/mem-alpha)
- Templates (source of truth): .agents/skills/engram-factory/assets/templates/ — read
  brief.md, analysis.md, plan.md, tasks.md, build.md, verification.md, review.md,
  decision.md, release.md. Current shapes: brief (13 sections: ask, problem, outcome
  measures, non-goals, MoSCoW, constraints, decision ledger, delegated-to-analysis, open,
  owner ratification, owner suggestions, handoff), analysis (13 sections: intent, story,
  evidence, decisions, requirements, acceptance, scope boundaries, verification stance,
  reviewer applicability, risks, glossary, environment inventory), plan (9: approach, new
  decisions, steps, contracts/compat, blast radius, verification map, recovery/rollback,
  glossary deltas), tasks (6: task list, execution rules, coverage, delivery checklist,
  review rework tracking, glossary deltas).
- Skills that drive them: .agents/skills/engram-factory/SKILL.md and
  .agents/skills/engram-frame/SKILL.md (frame = pre-factory owner interview → brief).
- REAL JOB OUTPUTS (evidence — read at least four):
  .engram-factory/jobs/002-expose-agent-skills/{brief,analysis,plan,tasks}.md,
  .engram-factory/jobs/006-zstd-advisory-remediation/ and 007-clippy-cleanup/
  (small jobs — did the template force disproportionate ceremony?),
  .engram-factory/jobs/003-light-team-mode/ (team already tried to reduce weight — learn),
  .engram-factory/jobs/010-project-bound-branch-indexing/ (recent). Raw asks live in
  .engram-factory/briefs/. Check history/cycle-*/ dirs for rework-loop evidence.

## Diagnose (with line counts and quotes)
1. Where do templates force boilerplate small jobs don't need? Which sections get filler?
2. Where do they UNDER-specify (causing rework loops — look at rework tracking + cycle dirs)?
3. What makes jobs slow: which artifacts are written before they can be right? Which
   sections duplicate across brief/analysis/plan/tasks? Trace ONE requirement through all
   four files of job 002 and count the restatements.
4. What's missing for machine-gating: which prose fields should become schema-checkable
   (rule IDs, gate names, file paths, checkboxes, coverage matrix) so CI can verify the artifact?

## Produce (Q2–Q5) — evolve, don't revolt; mark what you KEPT and why
- **Brief (intent framing)**: enhanced template. Keep MoSCoW + decision ledger strengths;
  add "constitution rules touched" declaration, problem-vs-symptom separation, and a
  size-tier field (S/M/L) that scales downstream ceremony. Full template.
- **Spec (today's analysis.md)**: restructured. Intent traceable to brief; requirements as
  testable statements with IDs (REQ-1…); acceptance criteria bound to named gates; evidence
  with file:line citations; scope boundaries as explicit out-list. Full template + ID scheme.
- **Plan**: every step carries touched constitution rule IDs, blast radius as concrete
  paths, verification per step (gate or command), rollback. Steps reference REQ IDs — no
  orphan steps. Full template.
- **Tasks**: checkbox-first, machine-checkable (task IDs → plan steps + REQ IDs, gate-to-run
  per task, REQ×task coverage matrix CI can verify is total). Full template.
- **Tiering table**: S/M/L job → required/optional/omitted sections (the direct noise fix).
- **Gate-ability checklist** per artifact: what CI could mechanically verify (frontmatter
  schema, REQ↔step↔task referential integrity, coverage totality, no TBD/stub markers in
  ratified sections — tie to constitution RULE-3/4/5).

## Rules
- BEFORE writing, run and cite actual results (or "index empty"):
  ./target/release/engram-mcp recall --query "factory job rework noisy template" --json --limit 5 --storage /home/videogamer/.engram --project agentzero
  ./target/release/engram-mcp recall --query "light team mode factory" --json --limit 5 --storage /home/videogamer/.engram --project agentzero
  (Parent already saw hit_count 0 on template queries — record honestly whatever returns.)
- Write full markdown to:
  /home/videogamer/projects/mem-alpha/docs/research/software-constitution/02-sdd-templates.md
  (write tool, absolute path). Date 2026-10-31. Structure: diagnosis (real quotes/line
  counts) → four templates → tiering table → gate-ability checklist.
- Analysis only — do NOT modify real templates or skill files.
- Templates stay markdown-first (agents fill them) but frontmatter/ID conventions must be
  regex/JSON-schema checkable. Under ~450 lines.
