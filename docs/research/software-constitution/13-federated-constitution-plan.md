# Plan: Federated domain constitution (product / architecture / build / testing / ci)

Date: 2026-11-01. Status: PROPOSAL — owner ratification required.
Inputs: 13a-federation-precedents.md (gpt-6.1-sol), 13b-domain-constitution-design.md
(gpt-5.6-terra), 13c-adversarial-review.md (gpt-6.1-terra — verdict: proceed-with-changes).
Owner's fixed shape: root `docs/constitution/00-INDEX.md` intact; five domain
sub-constitutions each with their own `00-INDEX.md` and rails, forward/backward
referenced; GENERIC for any software engram builds — including engram itself.

## 1. The one correction all three analyses force (additive, not relocating)

The owner's "Others remain intact" is load-bearing and literal: current rails
01–09 STAY at root with their IDs, histories, and enforcement claims. Domain
packs are NEW, SUBORDINATE parts of ONE constitution — not five independent
constitutions (13c's central finding: several silent-omission failure modes
appear only if rails are moved or packs claim independence). Root keeps:
universal rules, the six rule classes, DEVIATIONS, DECAY-WINDOWS, amendment
policy. Every pack's INDEX declares itself subordinate to the root INDEX.

## 2. Division of authority (13a's test, applied)

Federate by DECISION AUTHORITY, not by which tool checks compliance:

- **product/** — owns scope, outcome, and functional obligations (what the
  software is for; which behaviors are in/out)
- **architecture/** — owns structure: boundaries, ownership, component
  coherence, the shape new code conforms to
- **build/** — owns making: build reproducibility, dependency hygiene,
  packaging/distribution obligations
- **testing/** — owns proof: what counts as demonstrated (fixtures, coverage
  obligations, the measurement discipline)
- **ci/** — **enforcement view only** (13a): registers which gates enforce
  which rules on every push; owns NO product/build/testing rules. A rule has
  exactly one home; CI cites it.
- Security and release/operations are NOT new packs (13a flagged them as the
  gap): security obligations live in the packs they constrain (architecture
  boundaries, build supply chain) cited with `security:` refs; release/ops
  obligations live in build/ + product/. Documented in each INDEX.

## 3. Reference grammar (forward/backward, auditable)

Rule IDs stay globally unique (existing prefixes; new domains get their own
prefixes: PRD-, ARC-, BLD-, TST-, CIE-). Typed references on rule lines:

- `governs: <rule-ID>` — authority flows root→pack (acyclic; gate-checked)
- `enforced-by: <gate or rule-ID>` — a rule names its enforcement (ci/ pack
  holds the reciprocal registry)
- `refines: <rule-ID>` — a pack rule narrows a root rule locally
- plain `see:` — information refs, bidirectional, never load-bearing

The no-orphan rule generalizes RULE-6 both ways: every pack rule refines or
governs something; every root rule cited by a pack resolves; reciprocal
`enforced-by` entries must exist in ci/.

## 4. Generic domain-pack contract (the "any software" requirement)

A pack is instantiable from a template — this is what engram ships to other
repos and applies to itself:

```
<pack>/00-INDEX.md   — subordination declaration, scope charter (in/out),
                       rule table (ID, statement, refs, gate), owner
<pack>/NN-*.md       — rails (same rail format as root)
```

Root gains a machine-readable catalogue (`constitution-catalogue.json`:
rule-ID → pack → path → gate) — the single source every tool reads.
Genericity rule (13c #5): product-specific rules (engine neutrality, surface
parity) live in engram's OWN packs, never in the template.

## 5. Tooling changes (all additive)

1. `check_constitution.py`: recursive topology discovery (root + packs),
   catalogue validation, authority-DAG check, reciprocal-ref check
2. `check_job_artifacts.py`: rail-ID scan reads the catalogue (already needs
   recursion — 13c #4 confirmed the current glob misses subdirs)
3. Gates/registries: paths unchanged; ci/ pack's INDEX links them
4. `references/context.md`: analyzer/planner read their pack INDEXes

## 6. Migration (each step green, from 13b, tightened)

1. Freeze + inventory: catalogue schema + fixtures; no prose moves
2. Readers first: check_constitution learns packs/catalogue (backward-compat:
   zero packs is valid)
3. Create the five pack INDEXes + first domain rails, cross-referencing
   EXISTING root rules by ID (additive only)
4. Move enforcement claims into ci/'s registry view (citations, not copies)
5. Ship the pack template as the generic artifact; self-host check: use it to
   instantiate a fresh pack for a scratch repo, and audit engram's own
   constitution with it

## 7. Self-hosting (the paradox, answered)

The recursion lands where it always does: the constitution's own gates are
components in the components registry and rules in ci/ — build/ gates the
gate-builders, the root amendment policy governs pack charters. One cycle is
declared, named, and gate-checked rather than denied (13c #6).

## 8. Ratification asks

1. Additive-only restructure (rails 01–09 stay at root) — confirm
2. The five-pack cut with CI as enforcement view — confirm or add packs
3. Reference grammar (governs/enforced-by/refines/see) + catalogue — confirm
4. Generic pack template as a shippable engram artifact — confirm
5. Migration order (readers-first, all-green) — confirm
