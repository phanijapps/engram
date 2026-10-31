# Brief: Comparative critic (gpt-6.1-terra) — grade both generations against the exemplar bar

ROLE: You are the harsh reviewer. Compare three generations of engram's SDD
artifacts against what a top-tier engineering org would accept, and define the
merge criteria for the redesign.

## Read (cwd = /home/videogamer/projects/mem-alpha)
- v0 (rich prose, pre-factory rigor): .engram-factory/jobs/002-expose-agent-skills/analysis.md
- v1 templates (ceremony era): git show 5fa176ce~1:.agents/skills/engram-factory/assets/templates/analysis.md
- v2 templates + real output: .agents/skills/engram-factory/assets/templates/spec.md
  and .engram-factory/jobs/011-en004-fail-closed/{spec,plan,tasks}.md
- The gates that constrain the redesign: tools/ci/check_job_artifacts.py and
  .agents/skills/engram-factory/scripts/factory.py validate_artifact (lines ~94-130)

## Deliver (to docs/research/software-constitution/07-comparative-verdict.md)
1. Score each generation (v0/v1/v2) per artifact on: reviewer velocity (can a
   staff engineer extract the argument in 60 seconds?), auditability (can a gate
   or auditor verify claims?), honesty (does the format invite filler?),
   noise-per-fact. 1-5 each, with line-level evidence.
2. What v2 GAINED vs v0/v2 LOST vs v0 — name the specific regression that makes
   v2 feel like homework (the owner's complaint).
3. Merge criteria: the exact properties the redesigned templates must hold
   (numbered, testable) so we can accept the redesign without losing the
   enforcement layer. Include which check_job_artifacts validations may need
   loosening (e.g. matrix format) vs which are sacred.
4. One worked micro-example: the SAME small requirement (fail-closed gate path
   check) written in v2 style vs exemplar style, side by side.
Under ~250 lines. Date 2026-10-31. Analysis only — do NOT modify anything.
