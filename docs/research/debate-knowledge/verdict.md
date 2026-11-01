# Verdict draft — knowledge structure (for red-forman severity review)

Date: 2026-11-01. Coordinator decision drafted by the orchestrator from
position-A (monolithic) and position-B (modular); red-forman's severity-tagged
review of THIS document is the binding decision.

## The decision: modularize the two large files; keep the five small ones as-is — with a manifest discipline and a size trigger

### Per file

| File | Decision | Reason |
|---|---|---|
| architecture/knowledge.md (304 ln / ~6k tok) | **SPLIT** into `knowledge/` topic files + `knowledge/INDEX.md` manifest | Absorbed overview+reference+domain summary is three distinct topics; an analyzer writing about retrieval needs the seam, not the launcher story; an estimated ~6% of a 100k window **when loaded in full**; observed routing today reads indexes + relevant rules, so this is a selective-loading DESIGN CHOICE for tail-window models and per-seam jobs — not a demonstrated current failure |
| product/knowledge.md (297 ln / ~4.4k tok) | **SPLIT** into `knowledge/` topic files + manifest | Absorbed full product doc: mission, substrates, surfaces, capability tables are separately consumable; same selective-loading math |
| dev, testing, ci, security, release (12-14 ln) | **KEEP monolithic** knowledge.md | At ~160 tokens each, splitting is ceremony; they grow → same trigger applies |

### The manifest discipline (B's full contract adopted, position-B lines 95-111)

Every `knowledge/` directory carries `INDEX.md`. Per fragment the manifest
records: purpose, token estimate (measured, labeled estimate), **prerequisites
(companion fragments that MUST load with it)**, and **task routes** (which
factory stage/seam loads it). The manifest also declares one **full-read
route** (the equivalent of the old monolith, for tasks that need everything).
Validation: every fragment listed exactly once; every prerequisite resolves
to a listed fragment; every inbound reference from the pack 00-INDEX resolves
(pack links the manifest, never fragments directly). This checks completeness
of task context, not just inventory: an agent following a route cannot omit a
required companion (policy, engine boundaries, surface parity) without the
manifest naming the gap.

### The size law (THIS DECISION's proposal — not attributed to either advocate)

One rule, one measurement, one precedence:
- **Trigger**: a monolithic knowledge.md MUST split when
  `tokenizer-measured tokens > 3,000` (tokens rule; line counts are advisory
  signal only). Method: measure with the project tokenizer at split time.
- **Precedence**: tokens first; observed task cost (missed constraints,
  follow-up reads in factory jobs) can trigger a split EARLIER but never
  blocks one past the token threshold.
- **Enforcement**: advisory lint at 2,400 tokens (early warning), blocking at
  3,000 (the gate checks monolithic files; manifests are exempt per-fragment
  but the full-read route's total is reported in the manifest).

### Diagrams fold in (owner-approved proposal)

Diagrams count toward context and follow the same rule:
- `architecture/diagrams/`: context (C4-L1), containers (C4-L2), substrates
  flow — one Mermaid file each, listed in the architecture knowledge manifest
- `dev/diagrams/`: components-core (C4-L3), sequences (write/recall/code-
  index), build pipeline — listed in the dev manifest (dev stays monolithic
  knowledge.md until it grows; diagrams live as files, linked from it)

### Migration contract (exactly one canonical home)

The split change MUST, in the same commit: (a) move content to `knowledge/`
fragments + manifest; (b) update every inbound reference (pack 00-INDEX,
context.md routing, cross-pack links) to the manifest; (c) DELETE the old
knowledge.md or replace it with a ≤5-line non-duplicating redirect to the
manifest; (d) account for all original content (fragments + manifest cover
every section of the old file — nothing silently dropped). Validation: no
reference resolves to a removed file; no fragment duplicates another's
canonical content.

### Gate enforcement

Pack structural contract extends: IF `knowledge/` dir exists THEN `INDEX.md`
manifest required listing every fragment; monolithic knowledge.md allowed
only under ~150 lines (advisory lint, DW window before blocking).

## What each advocate got right (and wrong)

- A right: coherence cost is real; one mental model per pack; progressive
  loading already exists at INDEX/rules level. A wrong: "6% is fine" assumes
  big-window models only; the absorbed-docs reality makes architecture/
  knowledge.md three documents stapled together, not one coherent unit.
- B right: job context is the unit; selective loading is the right default;
  manifest discipline answers coherence. B wrong: splitting the five tiny
  packs (conceded in structure above).
