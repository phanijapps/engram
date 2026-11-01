---
name: engram-constitution
description: Build or grow a Software Constitution for ANY repository — grounded, evidence-cited domain packs (product/architecture/build/testing/ci/security/release) under a root INDEX. Follows the evidence→counterexample→gate-verification→ratification loop; every rule must be false for a different codebase. Trigger on "build a constitution", "add domain rules", "ground the packs", "constitution for this repo", or when adopting engram to govern a new codebase (including engram itself).
---

# engram-constitution

Turn a repository's ACTUAL law — the invariants its code, tests, and docs
already live by — into a federated, gate-linked constitution. You are the
analyst-drafter; the OWNER is the legislator. Nothing you draft is law until
ratified.

## Non-negotiables (BCP 14)

1. Every rule MUST be **false for a different codebase** — if it reads like
   fortune-cookie advice, delete it and find the real invariant.
2. Every rule MUST carry **repository evidence** (`file:line` or command).
3. Every rule MUST name a **counterexample** — the concrete change that
   would violate it. No counterexample, no rule.
4. An enforcement claim MUST be **verified against the gate's code** (read
   it, run its fixtures) — a catalogue that says "enforced-by" is a claim,
   not a proof. Doc-says/gate-does drift is the #1 failure mode.
5. One rule, one home: domain packs refine root rules (`governs`/`refines`/
   `enforced-by` refs); CI is an enforcement VIEW, never a rule home.
6. Nothing lands without owner ratification; the factory journal is the
   receipt.

## The loop (per repository, per pack)

### 1. Bind revision and authority
Record the exact revision you analyzed (commit SHA) and where authority
lives (owner channel). Un-rebased worktrees need direct source inspection.

### 2. Collect evidence — code and docs SEPARATELY
Code: workspace manifests, entry points, the real seams (who calls whom —
engram's code graph / `symbol_context` where available; grep otherwise).
Docs: product/mission docs, ADRs, existing rails, CI workflows, registries.
Docs are LEADS, not inventory — reconcile before drafting (count the
crates, read the globs; expect the docs to be stale somewhere).

### 3. Draft rules with counterexamples (the anti-generic test)
For each candidate: smallest BCP 14 obligation + evidence + the violating
change + which root rule it refines + gate that could enforce it. Start
from OBSERVED patterns ("SQL only in named engine zones"), not aspirations.

### 4. Verify enforcement claims
Read each gate's code; run its positive AND negative fixture; record what
it actually checks vs what the rule says. Partial enforcement is stated as
partial — never smoothed.

### 5. Propose for ratification
Deliver: rule table (ID | statement | refs/enforcement | evidence), the
discrepancies found (stale docs, gate holes, overclaims — these are
findings regardless of adoption), and the catalogue entries. Owner
ratifies; only then do packs/catalogue change; commit with the journal.

### 6. Distill the accepted revision
If the repo has engram: distill the ratified packs with source discipline
(stable source_id, exact-sha keys) so future sessions can query the law.

## House format (per pack)

```
<repo>/docs/constitution/<pack>/00-INDEX.md
  SUBORDINATE line (one constitution, one root)
  ## Charter  (what this pack owns — one paragraph, domain-specific)
  ## Rules    (table: ID | statement | refs/enforcement | evidence)
  ## Notes    (enforcement honesty, partial claims)
```
Prefixes: PRD/ARC/BLD/TST/CIE/SEC/REL (or domain-appropriate, globally
unique). Root INDEX stays authoritative; packs are additive.

## Anti-patterns

- Generic garbage ("code MUST be testable") — fails the false-elsewhere test
- Rules without counterexamples — unfalsifiable wishes
- Trusting the catalogue over the gate code (the EN-004 lesson: rail said
  fail-closed, gate silently skipped — the drift detector must READ code)
- Moving root rails into packs during grounding — additive only
- Drafting aspirations the code doesn't live by yet — those are proposals
  for CHANGE, labeled as such, never smuggled as law
- Landing rules without the owner's ratification
