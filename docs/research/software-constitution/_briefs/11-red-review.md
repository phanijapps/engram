# Brief: red-forman review — spec.md + plan.md proposals

You are reviewing TWO proposal files as the adversarial reviewer. Be brutal,
concrete, line-cited. No praise padding. Write ONE file:
docs/research/software-constitution/11-red-review-spec-plan.md

## Files under review (cwd = /home/videogamer/projects/mem-alpha)
- docs/research/software-constitution/proposed-templates/spec.md (v4.2:
  60-second case, user scenarios, EARS REQs w/ WHEN-THEN + delta tags,
  NEEDS CLARIFICATION, interfaces/quality/blast-radius lines, Gherkin ACs,
  evidence/decisions/assumptions, definition of ready)
- docs/research/software-constitution/proposed-templates/plan.md (v2:
  approach argument, high-level + technical architecture w/ Mermaid, STEP
  table, acceptance-to-evidence map, contracts/blast radius)

## For EACH file answer
1. Would a staff engineer actually write this, or perform it? Where does it
   still smell like a form — name lines.
2. Is anything DUPLICATED between spec and plan (the lane: behavior in spec,
   structure in plan — violations?)
3. Is anything MISSING a real reviewer needs (cite the section that should
   exist but doesn't)?
4. Do the worked examples teach or mislead (could an agent copy them into
   cargo-cult filler)?
5. Machine contracts: any frontmatter/ID issue that would break
   factory.py validate_artifact or check_job_artifacts.py?
6. Verdict per file: SHIP / FIX (list exact edits) / RETHINK.
Under 120 lines total. Date 2026-11-01. Review only — modify nothing.
