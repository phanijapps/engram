---
name: constitution
description: Build, grow, and MAINTAIN a Software Constitution for ANY repository — greenfield (interview) or brownfield (code + docs + interview) — with 8 domain packs (product/architecture/dev/testing/ci/security/release/platform), knowledge/rules split, one rule per file, DERIVED catalogue (never hand-edited), health detection, module sub-constitutions for monorepos. Self-maintaining: lane 1 derives, lane 2 detects and advises, lane 3 drafts only when the owner asks. Trigger on "build a constitution", "constitution health", "split a pack", "add a module", "ground the packs", or when adopting any codebase to govern.
---

# constitution

Turn a repository's actual law — or a greenfield owner's intent — into a
federated constitution: knowledge and rules in ONE tree, one rule per file,
catalogue-machined. You are the analyst-drafter; the OWNER
legislates.

## Layout (this skill)

```
assets/templates/    root-INDEX · pack-INDEX · knowledge · rule · catalogue
references/          pack-charters (the 8-pack cut, merge rules)
                     (the mechanical subset) · evidence-mining (brownfield)
                     · interview-guide (greenfield)
scripts/             scaffold.py (greenfield|brownfield|module skeleton) ·
                     derive_catalogue.py (lane 1: regenerate the registry) ·
                     health.py (lane 2: detect size law, drift, orphans) ·
                     query.py (search rules, knowledge, runbooks by terms) ·
                     graph.py (build/validate/visualize the knowledge graph) ·
                     validate.py (structure + catalogue) · lint.py (length)
evals/               test_scaffold.py (6 tests)
```

## Mode 1 — GREENFIELD (interview)

No code exists; the constitution encodes INTENT.

1. `python3 scripts/scaffold.py --repo <path> --mode greenfield [--packs ...]`
   (merges allowed — see references/pack-charters.md)
2. Interview per `references/interview-guide.md`: option-based, <= 4
   substantive questions per round, one pack at a time. The NOT-question is
   mandatory (what this software is NOT).
3. Fill knowledge.md from answers (descriptive register, marked
   intent-until-code). Convert outcome/never answers into rules via
   `assets/templates/rule.template.md` — EARS + BCP 14 + counterexample
   from the owner's own scenario.
4. Read the whole draft back; owner ratifies; then `scripts/validate.py` +
   `scripts/lint.py` clean + `scripts/derive_catalogue.py <repo> --write`.

## Mode 3 — MODULE (monorepo sub-constitutions)

`python3 scripts/scaffold.py --repo <path> --mode brownfield --module <name>`
creates `docs/constitution/modules/<name>/` — a sub-constitution for a
module with its own decision authority (microservice, independently
published package, distinct platform). The per-module×concern test decides
which packs a module owns: a deployable service gets product/architecture/
platform/release; a monolith module gets product rules only; a shared
library gets none (root law covers it). Module rules `refine:` root rules;
root architecture owns module boundaries, never internals.

## Self-management (the three lanes)

**Lane 1 — DERIVED (automation executes, no approval):**
`derive_catalogue.py <repo> --write` regenerates the catalogue from
rules/*.md scans. The gate proves the committed catalogue matches the
derivation. Humans never hand-edit the registry.

**Lane 2 — DETECT + ADVISE (automation surfaces, the owner decides):**
`health.py <repo>` emits findings: size law crossings (>=2,400 advisory,
>3,000 blocking), orphan fragments, uncatalogued rules, incomplete packs,
undeclared modules. The session agent reads findings and ADVISES the owner
with evidence — never a drafted diff. If the owner says no, the finding
stays open and visible.

**Lane 3 — HUMAN-ORIGINATED (the skill drafts only when asked):**
When the owner says "split testing" or "add platform rules", the skill
runs its mining/interview analysis and drafts with REJECTED ALTERNATIVES
listed. Structural changes require verified authorization: an L3
ratification bound to the exact revision, or a signed owner instruction.
The gate classifies changes from the diff, not the commit message.

## Mode 2 — BROWNFIELD (code + docs + interview)

1. `python3 scripts/scaffold.py --repo <path> --mode brownfield --docs <files/dirs>`
2. Mine per `references/evidence-mining.md`: manifests → seams → tests →
   docs (leads, not inventory). Absorb real knowledge into knowledge.md.
   Expect at least one doc-vs-code discrepancy — finding it is a deliverable.
3. Draft rules with the anti-generic test (FALSE for a different codebase),
   evidence (file:line), counterexample (the violating change), enforcement
   honesty (read the gate code; partial is stated as partial).
4. Interview only for what code+docs cannot answer (scar-born rules, debt
   vs law). Then ratify + validate + lint.

## Runbooks (procedural knowledge — a fourth content type)

Each pack may carry `runbooks/` — procedural documents (numbered steps, one
instruction per step) for diagnosing and mitigating known failure modes.
Runbooks are versioned: each carries `version`, `validated against` (the
revision it was last tested on), and a change log. After every incident the
runbook MUST be updated with what actually worked (increment `version`);
the post-incident checklist in the template enforces this. Runbooks are
scar-born: they start from the first incident and grow from real experience,
never from speculation. The constitution skill drafts them in the
imperative steps.

## Knowledge graph (OKF frontmatter)

Every constitution artifact carries OKF-style YAML frontmatter with typed links:

```yaml
---
type: rule | knowledge | runbook | index
id: PLT-001
title: Per-project storage isolation
description: One-line summary
pack: platform
status: draft | stable | deprecated
tags: [storage, isolation]
created_on: 2026-11-01
forward_links:
  - { target: PRD-101, relation: refines }
  - { target: check_distribution_drift, relation: enforced-by }
backward_links:
  - { target: SEC-601, relation: refined-by }
semantic_links:
  - { target: knowledge/platform/storage.md, relation: described-by }
---
```

The link kinds: `forward` (this node points to another), `backward` (reverse
edges, validated for reciprocity), `semantic` (cross-domain "see also").
Build and validate with:

```bash
python3 scripts/graph.py <repo> --validate   # check link integrity
python3 scripts/graph.py <repo> --dot        # Graphviz DOT output
python3 scripts/graph.py <repo> --json       # full graph as JSON
```

## Searching the constitution

`python3 scripts/query.py <repo> "<terms>" [--pack <name>] [--type rules|knowledge|runbooks|all]`

Searches the derived catalogue (rules), knowledge manifests (fragments with
prerequisites), and runbook triggers. Returns matches sized for progressive
loading — the agent loads only what the task needs. This IS the search
capability; no separate skill is needed.

## Non-negotiables (both modes)

1. Every rule MUST be false for a different codebase.
2. Every rule carries evidence (brownfield) or owner-intent marker (greenfield).
3. Every rule names a counterexample — no counterexample, no rule.
4. Enforcement claims are verified against gate code, never assumed.
5. One rule, one home: `rules/<ID>.md`; ci/ is an enforcement view only.
6. Nothing is law until the owner ratifies; aspirations are labeled proposals.

## Self-host

This skill builds constitutions for any repository. The reference
implementation is the engram project: https://github.com/phanijapps/engram
(docs/constitution/).
