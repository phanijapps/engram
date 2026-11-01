# Brief: Plan-template craft research (gpt-6.1-sol) — ISO 42010 + architecture views

ROLE: Research how a technical PLAN document should present architecture, then
design the section structure for engram's plan template.

## Study (fetch, extract principles)
- ISO/IEC/IEEE 42010 (architecture description): stakeholder/concern/viewpoint/
  view model; what an architecture description MUST contain; viewpoint framing
  (each view addresses identified concerns for identified stakeholders).
  Wikipedia + official synopsis are fine; cite what you used.
- Google design-doc practice + Amazon 6-pager (already partially covered in
  docs/research/software-constitution/05-spec-craft-analysis.md — read §plan):
  the "approach" narrative, alternatives-with-tradeoffs.
- How mature orgs structure "technical plan" vs "spec": the plan is WHERE the
  architecture lives (the spec is behavior; the plan is structure).

## Design problem (cwd = /home/videogamer/projects/mem-alpha)
The owner wants the plan to carry: proposed approach (narrative), HIGH-LEVEL
architecture, TECHNICAL architecture, FLOW diagrams. engram's plan is also a
governed artifact: STEP rows with REQ IDs + gates + rollback survive
(machine contract, read tools/ci/check_job_artifacts.py + factory.py
validate_artifact ~lines 94-130), tiering S/M/L scales content.
Read: docs/research/software-constitution/proposed-templates/plan.md (current
proposal), .engram-factory/jobs/011-en004-fail-closed/plan.md (real v2 output).

## Deliver -> docs/research/software-constitution/08-plan-craft.md
1. ISO 42010 distilled for plans: which viewpoints earn a view at S/M/L
   (system context, component/container, runtime flow/sequence, deployment),
   each mapped to the CONCERN it answers + the stakeholder who holds it.
2. Diagram-selection rules: when flowchart vs sequence vs C4 context/
   container; what NOT to diagram (per tier) — anti-ceremony.
3. The section skeleton for the redesigned plan (names + one-line purpose +
   tier marker), with the approach narrative BEFORE any diagram.
4. Diagram-as-code guidance: Mermaid (```mermaid fences render on GitHub,
   diffable, greppable) as the house style; kept-minimal rules (max boxes,
   one concern per diagram, worked examples teach).
5. What MUST NOT be lost: STEP table w/ REQ IDs + concrete paths + gate +
   rollback; AC-complete verification map; contracts/compat; recovery.
Under ~250 lines, date 2026-10-31. Analysis only — write ONLY the 08 file.
