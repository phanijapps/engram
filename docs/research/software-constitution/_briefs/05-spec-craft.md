# Brief: Spec-craft analysis (gpt-6.1-sol) — what makes brief/spec/plan/tasks EXCELLENT

ROLE: You are studying how world-class engineering organizations write intent and
design artifacts, then diagnosing why engram's v2 templates feel like "school
projects" (the owner's words) despite being technically rigorous.

## Study (fetch/skim, extract principles — not quotes)
- IETF RFC + PEP culture (Python PEP 1): problem→constraints->options->decision structure
- Amazon 6-pager narrative memos + working-backwards PR/FAQ: no bullets in the core
  argument, prose that carries reasoning; tables only for data
- Google design docs (public exemplars): context/scope, goals/non-goals, actual design
  narrative, alternatives considered with honest tradeoffs
- Rust RFCs (text of a few, e.g. RFC 114 or 2005): lightweight, decision-oriented
- Lightweight modern practice: one-pagers, PREs, "spec as README" culture

## Diagnose (read these files, cwd = /home/videogamer/projects/mem-alpha)
- .agents/skills/engram-factory/assets/templates/{brief,spec,plan,tasks}.md (v2)
- Real v2 output: .engram-factory/jobs/011-en004-fail-closed/{spec,plan,tasks}.md
- Real v1 output for contrast: .engram-factory/jobs/002-expose-agent-skills/analysis.md
Name the "school project" smell precisely: placeholder tables with no argument,
IDs masquerading as content, every section same shape, no narrative carrying WHY,
template comments visible in the body, filler rows (("| ... |") with <placeholders>).

## Deliver (to docs/research/software-constitution/05-spec-craft-analysis.md)
1. Eight-to-ten principles for each artifact (brief/spec/plan/tasks) distilled from
   the exemplar cultures — e.g. "narrative carries the argument; structure carries
   only what prose cannot (enumerable facts, schedules, matrices)".
2. A per-template diagnosis: what exactly reads as homework, with line evidence.
3. The tension named honestly: machine-gateable IDs/frontmatter vs senior-grade
   prose — what MUST stay structured (frontmatter, REQ/AC/GATE IDs, coverage matrix)
   vs what must become prose (approach, rationale, risks, scope narrative).
4. A verdict: keep-and-rewrite or replace, per template.
Under ~300 lines. Date 2026-10-31. Analysis only — do NOT modify templates.
