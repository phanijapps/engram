# Debate coordination brief — red-forman decides the knowledge.md structure

ROLE: You are the debate coordinator and final authority. Two advocates have
argued the structure of engram's constitution knowledge files. Read BOTH
positions AND the ground truth yourself, then render a binding verdict with
a concrete structure. Write ONE file:
docs/research/debate-knowledge/verdict.md

## Read (cwd = /home/videogamer/projects/mem-alpha)
- docs/research/debate-knowledge/position-A-monolithic.md
- docs/research/debate-knowledge/position-B-modular.md
- Ground truth (measure yourself): docs/constitution/*/knowledge.md sizes;
  the consumers (.agents/skills/engram-factory/references/context.md routing;
  analyzer/planner agents), typical factory-job artifact sizes
  (.engram-factory/jobs/011-en004-fail-closed/*.md as a sample)

## Your verdict must
1. Name the winner per file (or a hybrid) with reasons tied to the actual
   consumers: factory agents loading context per job at what budget.
2. Give the FINAL structure: exact file/directory layout for knowledge in
   every pack, including the manifest/index discipline (if modular: topic
   files + a knowledge/00-INDEX.md manifest with one-line purpose + size per
   fragment so agents load selectively; if monolithic: where the size cap
   sits and what happens when a file outgrows it).
3. Address BOTH large files (architecture 304 lines, product 297) AND the
   five small ones (keep as-is? same shape for consistency?).
4. Fold in the owner's already-approved diagram proposal: architecture
   carries high-level (C4 L1-L2 + substrates flow) under diagrams/, dev
   carries technical (C4 L3 + sequence/flowcharts) — state how diagrams
   interact with the verdict (diagrams count toward context too).
5. The enforcement: what the pack structural gate should require afterward.
6. Direct, no hedging. If an advocate argued something wrong, say so.
Under 120 lines. Date 2026-11-01. Write ONLY the verdict file.
