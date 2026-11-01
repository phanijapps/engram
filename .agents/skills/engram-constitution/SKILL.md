---
name: engram-constitution
description: Build or grow a Software Constitution for ANY repository — greenfield (interview) or brownfield (code + docs + interview) — into docs/constitution/ with knowledge/rules split, one rule per file, ASD-STE100 style, machine catalogue. Trigger on "build a constitution", "constitution for this repo", "ground the packs", or when engram adopts a new codebase to govern.
---

# engram-constitution

Turn a repository's actual law — or a greenfield owner's intent — into a
federated constitution: knowledge and rules in ONE tree, one rule per file,
STE-compliant, catalogue-machined. You are the analyst-drafter; the OWNER
legislates.

## Layout (this skill)

```
assets/templates/    root-INDEX · pack-INDEX · knowledge · rule · catalogue
references/          pack-charters (the 7-pack cut, merge rules) · ste-style
                     (the mechanical subset) · evidence-mining (brownfield)
                     · interview-guide (greenfield)
scripts/             scaffold.py (greenfield|brownfield skeleton) ·
                     validate.py (structure + catalogue) · lint.py (STE)
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
   `assets/templates/rule.template.md` — EARS + BCP 14 + STE + counterexample
   from the owner's own scenario.
4. Read the whole draft back; owner ratifies; then `scripts/validate.py` +
   `scripts/lint.py` clean.

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

## Non-negotiables (both modes)

1. Every rule MUST be false for a different codebase.
2. Every rule carries evidence (brownfield) or owner-intent marker (greenfield).
3. Every rule names a counterexample — no counterexample, no rule.
4. Enforcement claims are verified against gate code, never assumed.
5. One rule, one home: `rules/<ID>.md`; ci/ is an enforcement view only.
6. Nothing is law until the owner ratifies; aspirations are labeled proposals.
7. ASD-STE100 registers: procedural (rules, <= 20 words) and descriptive
   (knowledge, <= 25 words); lint clean before handoff.

## Self-host

This skill built engram's own constitution (docs/constitution/) — the
dogfood path is the reference implementation.
