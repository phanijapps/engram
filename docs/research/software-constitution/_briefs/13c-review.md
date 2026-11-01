# Brief: Adversarial review of the domain-constitution idea (gpt-6.1-terra)

ROLE: Attack the federated constitution design BEFORE it's built. The owner
wants: root intact + product/architecture/build/testing/ci sub-constitutions,
each with 00-INDEX + rails, forward/backward refs, GENERIC for any software
and self-hosting (engram builds itself). Write ONE file:
docs/research/software-constitution/13c-adversarial-review.md

## Read (cwd = /home/videogamer/projects/mem-alpha)
- docs/constitution/ (current 9 rails — the content that must survive)
- docs/contracts/constitution/v1/gates/registry.json + components/registry.json
- tools/ci/check_constitution.py, check_coherent_integration.py,
  tools/ci/check_job_artifacts.py (the constitution_rules_touched validation
  scans docs/constitution/0*.md — what breaks with subdirs?)
- docs/research/software-constitution/00-sdd-constitution-plan.md §1 (the
  original topology rationale + SCCT pin discussion)

## Attack vectors (answer each, evidence-based)
1. Circular references: product↔architecture↔build legitimately depend on
   each other — where does the ref DAG break, and does it matter?
2. Rule-class coverage: does EVERY sub-constitution need to address the six
   RULE classes, or only the root? What does the standard require?
3. Duplication bait: build vs testing vs ci overlap (unit tests in build?
   automation in ci?) — name the boundary that prevents the same rule in two
   packs drifting.
4. Tooling breakage list: every script/gate/registry/template path that the
   restructure would break, concretely.
5. Genericity trap: what in engram's current rails is product-specific
   (engine neutrality etc.) and must NOT leak into the generic pack template?
6. The self-hosting paradox: engram governing itself — where does the
   recursion bite (the constitution governing the constitution's own gates)?
7. Verdict: proceed / proceed-with-changes (list) / rethink.
Under ~200 lines. No repo edits.
