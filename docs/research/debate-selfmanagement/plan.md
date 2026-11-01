# Plan draft — self-managing constitution (for red-forman severity review)

Date: 2026-11-01. Synthesized from position-A (automate) and position-B
(human-directed); red-forman's severity review of THIS document is the
binding decision.

## The decision: a three-lane model — derive everything, detect and advise, never originate structure

### Lane 1 — DERIVED (automation executes, no approval, no discretion)

Bookkeeping that is a projection of law, never law itself. Regenerated on
every merge; humans never hand-edit:

| Artifact | Derived from | Command |
|---|---|---|
| `registry/constitution-catalogue.json` | scan `*/rules/*.md` (ID, statement, refs, gate) | `tools/ci/derive_catalogue.py` (new) |
| knowledge manifests — MECHANICAL FIELDS ONLY (token estimates, fragment inventory, generated-at) | scan `knowledge/*.md` + the human-authored `knowledge/meta.yaml` per pack | same command, manifest section |
| L3 evidence view in ci/ pack | events.jsonl rollup | same |

Manifest semantics stay HUMAN-AUTHORED: each pack's `knowledge/meta.yaml`
(fragment purposes, prerequisites, task routes, full-read route — the
knowledge-verdict contract) is the canonical source the owner edits; the
derived manifest merges meta.yaml with mechanical measurements and NEVER
overwrites semantic fields. Fragment membership or route changes are
lane-3 (owner-originated) — regeneration only re-measures.

The gate flips AFTER an owner-approved canonical-source migration: coverage
defined (universal rules in 00-INDEX included, not just */rules/*.md),
enforcement strength preserved as structured fields (fail-closed parsing,
duplicate-ID detection, bidirectional inventory: every rule file appears,
every catalogue entry resolves), negative fixtures proving the gate catches
omissions. Then `check_constitution.py` regenerates in-memory and FAILS on
diff vs committed — hand-maintained JSON becomes a build artifact (B's
concession, adopted with A's prerequisites).

### Lane 2 — DETECT + ADVISE (automation surfaces, the owner or the session agent originates)

Gates and a new `constitution_health.py` command detect conditions and
report them with evidence — **as findings, not drafted diffs**:

- size law crossed → finding names the pack, measured tokens, routes.
  RATIFIED POLICY (this plan, resolving the knowledge-verdict conflict):
  measure with the pinned tokenizer (version recorded in the gate);
  advisory finding at >= 2,400 tokens; ENFORCED FAILURE at > 3,000 tokens
  (strictly greater; 3,000 exactly passes). Lines are never enforced.
  CI advisory channel reports; the enforcement channel blocks. Boundary
  tests pin 2,399/2,400/2,400.01 and 3,000/3,000.01 behavior.
- manifest gaps (fragment not listed, prerequisite unresolved)
- rules without counters, gates without rules, orphan fragments
- EN-004-class doc-vs-code drift (rail says X, gate does Y)

Advisory surface: gate output (CI), `constitution_health` (runnable), and
**session advice** — the assistant in any working session reads health
findings and ADVISES the owner ("testing crossed the size law; want a split
analysis?"). This is the inversion the owner demanded: the system surfaces,
the human decides whether to ask for the analysis.

### Lane 3 — HUMAN-ORIGINATED (the assistant drafts only when the owner asks)

Structure and law: splits and their boundary taxonomy, new/changed rules,
pack merges, amendments. When the owner says "split testing", the
engram-constitution skill runs its analysis and drafts — one draft per
request, alternatives shown (B's objection to proposal-as-default adopted:
a prepared diff becomes the default answer to an unasked question).

### What this installs (build order)

0. Manifest-aware structural gate FIRST: check_constition accepts EITHER
   monolithic knowledge.md OR knowledge/ + meta.yaml + derived manifest
   (prerequisite resolution, inbound links, content-preservation accounting,
   negative fixtures) — ships in the SAME change as the first split, so the
   migration contract is executable
1. `derive_catalogue.py` + canonical-source migration + gate flip — week 1
2. `constitution_health.py` (detection lanes above; runs in CI advisory +
   on demand) — week 1
3. Session-advice wiring: health findings into the context the assistant
   reads at session start (index-mining hook extension) — week 2
4. L3 `ci.run` event emission (health runs recorded in the audit chain) — later
5. The engram-constitution skill gains `--health` mode: runs detection, prints
   findings, and (only on owner request) drafts the change — later

### Guards (both positions' failure modes)

- No cron/spam: detection runs where eyes already are (CI, session start)
- No silent structure — authorization is VERIFIED, not asserted: lane-3
  structural changes require owner-origin evidence the gate can check
  independently: a ratification record in `.engram-factory/jobs/<id>/` (the
  L3 `job.approved` event bound to the exact source revision + artifact
  hashes) OR a signed owner instruction file committed with the change. The
  gate classifies changes from the DIFF (rules/ added|removed|edited,
  knowledge structure changed), not the commit message, and FAILS a
  structural diff lacking a matching, unexpired ratification (bound to the
  same revision; invalidated if the patch changes after approval).
- Alternatives discipline: when the skill drafts a structural change, the
  draft MUST list rejected alternatives (counterexample discipline applied
  to structure itself)

## What each advocate got right

- A right: derive-don't-maintain; the advisory inversion; fast-lane guards.
  A wrong (per B, adopted): filed drafted diffs as the default flow — the
  framing-power problem is real.
- B right: proposal-is-structure; approval asymmetry; detection≠proposal;
  session-agent advice as the correct advisory surface. B conceded: all of
  lane 1 (correctly — it's bookkeeping, not structure).
