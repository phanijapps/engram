# Brief: Design the domain-constitution restructure (gpt-5.6-terra)

ROLE: Produce the CONCRETE restructure design for engram's constitution into
the owner's shape — generic for ANY software project (engram will build other
software and itself with this). Design only — write ONE file:
docs/research/software-constitution/13b-domain-constitution-design.md

## The owner's shape (fixed constraint)
docs/constitution/00-INDEX.md + root artifacts intact, plus sub-constitutions:
product/ architecture/ build/ testing/ ci/ — each with its own 00-INDEX.md
and rails, cross-referencing forward/backward.

## Read (cwd = /home/videogamer/projects/mem-alpha)
- docs/constitution/ (all 9 rails + INDEX + DEVIATIONS + DECAY-WINDOWS)
- docs/contracts/constitution/v1/ (5 registries + schemas)
- tools/ci/check_constitution.py + check_job_artifacts.py (what parses rails)
- references/context.md (.agents/skills/engram-factory/references/context.md)
- docs/research/software-constitution/13a sister output may not exist yet —
  design from your own judgment.

## Design deliverables
1. Full file tree: root + 5 domain packs; where each current rail 01-09 lands
   (or splits); where DEVIATIONS/DECAY-WINDOWS live (root? per-domain?);
   where gates/registries live (per-domain subregistries vs central).
2. Reference convention: rule IDs stay globally unique (existing prefixes);
   forward/backward refs = rule-ID citations with direction semantics
   (e.g. "enforced-by CI rail", "governs architecture/03"). Define the
   no-orphan-both-ways rule (RULE-6 generalization across domains).
3. Root vs domain charter: root owns universal rules + rule classes + zero-
   deviation + evidence schema; each domain owns its rules + gates. A domain
   00-INDEX template so ANY project can instantiate a pack.
4. Genericity: a "domain pack" spec (required files, index shape, ref rules)
   making the whole thing a reusable template — engram-the-product ships it,
   engram-the-tool applies it to any repo it builds (incl. itself).
5. Migration plan: numbered steps, what moves, what renames, tooling changes
   (check_constitution recursion, artifact-gate rail scan, context manifest),
   zero broken refs, gates stay green throughout.
6. Self-hosting test: describe how the SAME structure serves (a) a fresh
   webapp repo, (b) engram building itself (dogfood path).
Under ~300 lines. No repo edits.
