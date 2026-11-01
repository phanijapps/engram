# Brief: Tasks-template review — lane-coherence lens (gpt-5.6-terra)

ROLE: Review the proposed tasks template for COHERENCE with the redesigned
lane: REQ (EARS, spec) -> AC (Gherkin, spec) -> TASK (tasks) -> gate (code)
-> evidence (verification.md). Verdict + concrete fixes. Review only — write
ONE file: docs/research/software-constitution/10-tasks-review-lane.md

## Read (cwd = /home/videogamer/projects/mem-alpha)
- docs/research/software-constitution/proposed-templates/tasks.md
- docs/research/software-constitution/proposed-templates/spec.md (v4.2:
  Gherkin ACs; test scenarios explicitly NOT in spec, NOT Gherkin)
- docs/research/software-constitution/proposed-templates/plan.md (v2:
  STEP table, architecture views)
- tools/ci/check_job_artifacts.py (what the gate actually regexes)
- .agents/skills/engram-factory/scripts/factory.py (validate_artifact + the
  tasks/build stage re-completion flow: tasks.md is re-authored by
  engram-builder at build stage with snapshot — does the template support
  that two-author lifecycle?)

## Review questions
1. Lane closure: does every construct in tasks.md have a clear upstream
   (spec REQ/AC, plan STEP) and downstream (gate, verification.md)? Any
   orphan concept with no producer or consumer?
2. The AC column: TASK rows reference REQ IDs today; with Gherkin ACs in the
   spec, should TASK rows also bind to AC (scenario) IDs? Which binding is
   load-bearing for coverage totality?
3. Two-author lifecycle: tasks-stage (engram-tasks) vs build-stage
   (engram-builder) re-completion — does the template body survive that
   flow? What must be re-filled vs preserved?
4. Gate inventory + verification handoff: is the "What did verification
   actually report?" section the right home, or should it be a pointer to
   verification.md to avoid duplicate evidence state?
5. Machine contract: which fields does check_job_artifacts.py actually need
   (literal REQ/TASK/STEP/GATE IDs) — is anything in the proposal decorative
   to the gate yet unhelpful to humans?
Output: verdict, findings with evidence, PATCH LIST (old -> new edits).
Under 150 lines, date 2026-11-01.
