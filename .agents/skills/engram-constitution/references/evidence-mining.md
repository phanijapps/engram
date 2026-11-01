# Evidence mining (brownfield: code + docs + interview)

## Mine order
1. Manifests: workspace members, package globs, CI workflows — the real inventory.
2. Entry points + seams: who calls whom; find the load-bearing indirections.
3. Tests + fixtures: what the code PROVES about itself — the best rule source.
4. Docs: mission, READMEs, ADRs/decision records — leads, not inventory.
   Expect at least one doc-vs-code discrepancy; finding it is a deliverable.

## The candidate-rule discipline
For each candidate: smallest BCP 14 obligation + evidence (file:line) +
COUNTEREXAMPLE (the concrete violating change) + which gate could enforce it.
A pattern you OBSERVE is a candidate; a pattern you WISH FOR is a proposal —
label proposals as proposals, never smuggle them as law.

## Enforcement honesty
Read the gate's code; run its fixtures if any. Record what it checks vs what
the rule claims. Partial enforcement is stated as partial. A catalogue
"enforced-by" is a claim, not a proof.

## Interview fill (the questions docs cannot answer)
- What behavior would you defend in an argument? (rule candidates)
- What did the last incident teach? (rules born from scars)
- What is debt you are paying down, not law? (decay window, not rule)
