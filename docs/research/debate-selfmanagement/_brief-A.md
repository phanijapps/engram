# Debate A — MAXIMAL automation: the constitution manages itself

ROLE: Argue for a SELF-MANAGING constitution: automation detects, proposes,
and (within boundaries) executes structural changes; the owner only ratifies.
Write to: docs/research/debate-selfmanagement/position-A-automate.md

## Ground truth (verify in repo, cwd = /home/videogamer/projects/mem-alpha)
- Size law exists: advisory 2400 / blocking 3000 tokens (verdict in
  docs/research/debate-knowledge/verdict.md) — but NOTHING detects or
  proposes the split; the owner must notice and prompt
- Catalogue is hand-maintained JSON
  (docs/constitution/registry/constitution-catalogue.json) — every rule
  addition edits it manually; it WILL drift
- Gates validate but never propose: check_constitution.py finds violations,
  suggests nothing
- The engram-constitution skill drafts only when invoked
- L3 events exist (job lifecycle); ci.run family declared but not emitted
- Factory pattern: proposals → owner gate → merge (the human-in-loop shape)

## Your case must cover
1. Derive-don't-maintain: catalogue auto-generated from rules/*.md scans
   (ID, statement, refs, gate) — the JSON becomes a build artifact; gate
   validates derivation, humans never edit it
2. Proposal engine: a constitution-health command (post-merge / scheduled /
   gate-finding-triggered) that runs the skill's analysis and FILES
   recommendations: "testing/knowledge.md crossed 2400 tokens — split
   proposal attached (fragments + manifest drafted)" — owner approves or
   rejects, never originates
3. Self-healing boundaries: what automation may do WITHOUT approval (derived
   artifacts, manifest regeneration, lint fixes) vs WITH approval (any law
   text, splits, rule changes)
4. The inversion the owner demands: the system advises (slack/inbox/job
   queue), the owner decides — enumerate the advisory surfaces
5. Failure modes of automation (auto-drift into wrong structure, proposal
   spam, owner rubber-stamping) and their guards
6. Concede: what genuinely must stay human-directed, and why prompting isn't
   always a failure mode
Under 150 lines. Write ONLY that file.
