# Brief: Plan-template redesign (gpt-5.6-terra) — architecture + flows, contracts intact

ROLE: Rewrite engram's plan template proposal so it carries the engineering
argument WITH architecture: proposed approach narrative, high-level
architecture, technical architecture, flow diagrams — while preserving every
machine contract. You write ONLY docs/research/software-constitution/
proposed-templates/plan.md (replace its content; keep it a proposal).

## Hard constraints (verified in code — do not break)
1. Frontmatter: JSON-per-line, starts with ---, union keys (schema_version 2,
   artifact "plan", job_id/cycle/status/author_role/profiles/source_snapshot/
   input_artifacts/updated_at + size_tier + spec_ref +
   constitution_rules_touched), {{job_id}} {{cycle}} {{profiles_json}}
   placeholders intact. Factory parser: .agents/skills/engram-factory/scripts/
   factory.py metadata() + validate_artifact.
2. STEP rows survive as a table: ID | imperative | REQ IDs | rule IDs |
   concrete paths | verify (GATE) | rollback. No orphan steps; every REQ
   covered. Verification map: every AC -> STEP + GATE.
3. Tiering: S minimal (approach + ONE diagram or none + steps), M/L ADD
   narrative/views (never blank scaffolding). Worked-example discipline: every
   table one realistic filled row; every diagram a COMPLETE worked example
   (use job 011: fail-closed engine-neutrality gate — gate scans declared
   paths, missing path -> named violation; fixture test; CI wiring).
4. Diagrams are Mermaid fenced blocks (```mermaid) — GitHub-renders, diffable.
   Choose per ISO-42010 concern: flowchart for control flow, sequenceDiagram
   for runtime interactions, C4-ish context graph for boundaries. One concern
   per diagram; label nodes with real engram paths where sensible.
5. Read first: proposed-templates/plan.md (current), 08-plan-craft brief's
   sister output may not exist yet — design from your own judgment +
   .engram-factory/jobs/011-en004-fail-closed/plan.md + spec.md proposal
   (proposed-templates/spec.md v4.2) for ID continuity.

## Structure to hit (owner's ask)
- Opening: the approach argument (design narrative, alternatives considered
  inline, why this design satisfies the spec's REQs — reference IDs in prose).
- High-level architecture: system context view (what's inside/outside the
  change boundary) — one Mermaid diagram + 3-5 lines of prose concern mapping.
- Technical architecture: component/container view + key runtime flow
  (sequence or flowchart) — the change's seam named in real paths.
- Steps/verification/rollback/contracts: the governed tables (constraint 2).
- A short header comment: proposal status + integration note.
Under ~120 lines total. The result must read like a staff engineer's plan
email with embedded diagrams, not a form.
