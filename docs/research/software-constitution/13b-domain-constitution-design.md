# Domain-Constitution Restructure Design

Date: 2026-11-01  
Status: proposed design only; no live constitution, registry, gate, or context-manifest change is made by this document.

## 1. Decision and invariants

Keep `docs/constitution/00-INDEX.md`, `DEVIATIONS.md`, and `DECAY-WINDOWS.md` at
root. Add five domain packs: `product`, `architecture`, `build`, `testing`, and
`ci`. A rule has one canonical home: a domain rail, never a copied rule body in
another pack. Root remains the charter and cross-pack directory, not a sixth
implementation domain.

The current six universal rule classes remain root-owned: RULE-1 through RULE-6.
Root also owns rule-class definitions, the zero-deviation mandate, decay-window
semantics, evidence-record minimum fields, permanent identifier policy, and the
central registry contract. A domain owns concrete rules for its concern and the
acceptance evidence those rules require. CI owns automation and evidence policy;
it does not take ownership of the product, architecture, build, or test rule it
enforces.

The current five JSON registries remain one authoritative, central set under
`docs/contracts/constitution/v1/`. Splitting them into per-domain registries
would duplicate global identities (`GATE-*`, components, distribution surfaces,
and event families) and make RULE-6 reconciliation cross-file by construction.
Each applicable entry gains a required `domain` field and retains its current
canonical rule references. Per-domain index tables are human-readable filtered
views, not machine-authoritative subregistries. Gate scripts remain in
`tools/ci/`; their registry entries remain the only rule-to-executable mapping.

`DEVIATIONS.md` remains root-only because an active exception violates the
universal RULE-5 regardless of the affected pack. `DECAY-WINDOWS.md` also remains
root-only because it is a temporal clause of a rule, not an exception; each row
adds the affected `domain` and canonical rule ID. Domain rails may link to their
rows but may not reproduce them.

## 2. Target file tree

```text
docs/constitution/
  00-INDEX.md                         # root charter, pack catalogue, universal rules
  DEVIATIONS.md                        # global active/closed exception record
  DECAY-WINDOWS.md                     # global temporal-clause record
  01-engine-neutrality.md              # permanent legacy redirect -> architecture/01
  02-surface-parity.md                 # permanent legacy redirect -> product/02
  03-canonical-implementations.md      # permanent legacy redirect -> split rule homes
  04-distribution-drift.md             # permanent legacy redirect -> build/04
  05-production-completeness.md        # permanent legacy redirect -> testing/05
  06-canonical-evidence.md             # permanent legacy redirect -> ci/06
  07-zero-deviations.md                # permanent legacy redirect -> ci/07
  08-coherent-integration.md           # permanent legacy redirect -> split rule homes
  09-principles.md                     # permanent legacy redirect -> split rule homes
  product/
    00-INDEX.md
    02-surface-parity.md
    03-transport-delegation.md
  architecture/
    00-INDEX.md
    01-engine-neutrality.md
    03-canonical-ownership.md
    08-component-coherence.md
    09-architecture-principles.md
  build/
    00-INDEX.md
    04-distribution-drift.md
  testing/
    00-INDEX.md
    05-production-completeness.md
    09-measurement-principle.md
  ci/
    00-INDEX.md
    06-canonical-evidence.md
    07-zero-deviations.md
    08-job-artifact-integration.md

docs/contracts/constitution/v1/
  implementation/registry.json         # central; entries carry domain
  events/registry.json                 # central; families carry owning domain
  gates/registry.json                  # central; gates carry domain + rule IDs
  distributions/registry.json          # central; surfaces carry domain
  components/registry.json             # central; components carry domain(s)
  schemas/                             # schemas updated for those fields
```

A redirect contains only title, canonical target link, and a historical note;
it contains no rule text and is excluded from the canonical-rule inventory. This
preserves the present permanent rail filenames and inbound links without creating
two homes. The root index lists redirects as compatibility paths and lists the
domain path as the canonical source. It must not call a redirect a current rail.

## 3. Current rail and rule mapping

The physical rail number travels with its rule family. Where an existing rail
contains different domain concerns, its stable rule IDs move independently;
there is no attempt to invent new IDs merely to make paths look sequential.

| Current rail | Canonical destination and owned rules | Reason for the cut |
|---|---|---|
| 01 Engine neutrality | `architecture/01-engine-neutrality.md`: EN-001..EN-005 | Neutral boundaries and permitted composition zones are architecture. |
| 02 Surface parity | `product/02-surface-parity.md`: SP-001..SP-004 | Supported user-facing transports and capability availability are product commitments. |
| 03 Canonical implementations | `architecture/03-canonical-ownership.md`: CI-001, CI-003, CI-004; `product/03-transport-delegation.md`: CI-002 | Canonical behavior and generated-contract source are architecture; wrapper delegation is a product-surface obligation. |
| 04 Distribution drift | `build/04-distribution-drift.md`: DD-001..DD-004 | Packaging, publication, install payloads, and source reconciliation are build/release concerns. |
| 05 Production completeness | `testing/05-production-completeness.md`: PC-001..PC-003 | Reachability checks, fixture boundaries, and ratified-artifact completeness are verification policy. |
| 06 Canonical evidence | `ci/06-canonical-evidence.md`: CE-001..CE-004 | The privileged workflow and its canonical evidence chain are CI governance. |
| 07 Zero deviations | `ci/07-zero-deviations.md`: ZD-001..ZD-004 | CI verifies the root exception and temporal-clause rules; root still owns their universal semantics. |
| 08 Coherent integration | `architecture/08-component-coherence.md`: OR-001..OR-003; `ci/08-job-artifact-integration.md`: OR-004 | Component identity and repository-shape truth are architecture; factory job layout is CI workflow hygiene. |
| 09 Principles | `architecture/09-architecture-principles.md`: PR-001..PR-005; `testing/09-measurement-principle.md`: PR-006 | Contract/boundary/policy principles direct architecture; measured-over-intuited directs experimental verification. |

This mapping changes no rule ID, gate ID, registry concept ID, or rule class.
A rule that names another pack's concern cites it; it does not restate it. For
example, product parity can depend on architectural ownership and build release
rules can require CI evidence without copying EN, CI, or CE requirements.

## 4. Reference convention and two-way orphan rule

### Canonical identifiers

- Rule IDs are globally unique and permanent. Existing prefixes (`EN`, `SP`,
  `CI`, `DD`, `PC`, `CE`, `ZD`, `OR`, `PR`) remain unchanged. New packs choose a
  unique project-wide prefix, not a pack-local number.
- Gate, component, event, concept, and distribution identifiers likewise remain
  globally unique. Paths are locators, not identities.
- Every canonical rule has an explicit heading anchor, owner pack, rule class,
  enforcement status, and `GATE-*` references. A canonical index is the source
  for resolving an ID to its path.

### Directed references

Use a linked rule-ID citation plus one of these verbs; a bare file-path citation
is explanatory only and cannot satisfy a constitutional dependency:

```markdown
- depends-on: [EN-003](../architecture/01-engine-neutrality.md#en-003)
- governs: [SP-001](../product/02-surface-parity.md#sp-001)
- enforced-by: [CE-002](../ci/06-canonical-evidence.md#ce-002)
- enforces: [DD-004](../build/04-distribution-drift.md#dd-004)
```

`depends-on` points from the consuming rule to the prerequisite owner.
`governs` points from a foundational rule to the narrower rule it constrains.
`enforced-by` points from a governed rule to the CI rule that defines enforcement
requirements; `enforces` is its reciprocal. The actual executable is named only
by `GATE-*`; the central gate registry maps that ID to its path and complete rule
set. A referenced rule is therefore usable even if its file is relocated later.

The **no-orphan-both-ways rule** generalizes RULE-6 across packs:

1. Every rule-ID citation resolves exactly once to a canonical rule and its
   anchor and owning pack exist.
2. Every normative cross-pack citation has the reciprocal declaration with the
   matching inverse verb (`depends-on`/`governs`, `enforced-by`/`enforces`).
3. Every canonical rule appears in its pack index, the root canonical catalogue,
   and at least one gate-registry rule list or an explicit `enforcement: review`
   record. Every `GATE-*` rule reference resolves back to the rule's declared
   gate list.
4. No redirect, registry entry, gate, pack, or canonical rule may be reachable
   only in one direction. The checker reports both the missing target and the
   missing reciprocal, rather than silently accepting a forward link.

Normative `depends-on` edges form a DAG. Reciprocal catalogue, governance, and
enforcement links are validated as paired metadata and are not dependency edges;
therefore their required backward link does not manufacture a logical cycle.
A desired new dependency cycle is resolved by extracting a root universal rule
or redesigning the responsibility boundary, never by adding reciprocal prose.

## 5. Root charter and reusable domain-pack specification

### Root `00-INDEX.md` must contain

1. scope and applicability; the six universal rule classes; RFC-2119 policy;
   root ownership of deviations, decay, IDs, evidence minimums, and registries;
2. a domain-pack catalogue (`name`, purpose, canonical index, owner, status);
3. a canonical rule catalogue (`rule ID`, canonical path, class, owner pack,
   `GATE-*`/review status), plus a separate legacy-redirect table;
4. registry and schema locations, reference grammar, no-orphan-both-ways policy,
   amendment policy, and conformance claims; and
5. a change rule: an amendment updates its canonical rail, index/catalogue,
   registry mappings, reciprocal references, and proof in one review.

It contains universal law, not a duplicate of domain requirements. Root rules
may constrain every pack; they must not embed an Engram engine, crate, transport,
or deployment assumption.

### Generic domain-pack contract

A project may instantiate a pack named for any stable concern. This design uses
five owner-required concerns, but the template has no Engram-specific rail names.
Each pack requires:

```text
<domain>/
  00-INDEX.md        # required: scope, exclusions, rails, refs, gates, owner
  NN-<topic>.md      # one or more required canonical rails
```

A pack index has these required sections: **Purpose and boundary** (what belongs
and explicitly does not), **Canonical rails** (rule IDs, paths, classes,
status), **Inbound/outbound rule references** (verb-qualified citations),
**Enforcement** (`GATE-*` links and review-only records), **Root obligations**
(deviation/decay/evidence links), and **History**. Rail files contain owner,
rule class, normative rules, enforcement boundary, cross-pack references, and
history. Optional explanatory files are not rails and cannot introduce rules.

A pack cannot redefine RULE-1..RULE-6, create a local exception file, define a
shadow registry, or copy another rule body. It may be absent only when its
purpose is genuinely out of scope; its absence is recorded in the root pack
catalogue. Thus a fresh project can use the same template with different rules,
while Engram-the-product can ship the template and Engram-the-tool can apply it
to every repository it builds, including this repository.

## 6. Migration plan (all commits remain green)

1. **Freeze and inventory.** Record the current 01–09 rule-ID-to-path map,
   present registry references, external documentation links, and all scripts
   that glob `docs/constitution`. Add a machine-readable canonical catalogue
   schema and fixtures before moving prose. No rule changes in this step.
2. **Make readers topology-aware first.** Extend `check_constitution.py` to
   discover root artifacts, each `*/00-INDEX.md`, and canonical rails recursively;
   validate the root catalogue, IDs, anchors, reciprocal references, and legacy
   redirects. It must accept the existing flat layout until the move commit.
3. **Update downstream scanners.** Change `check_job_artifacts.py` from
   `Path("docs/constitution").glob("0*.md")` to the root canonical catalogue
   (or the same validated discovery helper), so `constitution_rules_touched`
   validates all pack rules rather than only root filenames. Update any other
   rail glob, registry schema validator, CI workflow path filter, and documented
   command to use the helper. Add positive nested-rule and negative broken-ref
   fixtures.
4. **Add the pack skeletons and root catalogue.** Introduce five indexes with
   boundaries and empty rail tables while the flat rails remain canonical.
   Central registries add `domain` only after schemas and readers accept it;
   no registry is split or temporarily duplicated.
5. **Perform one atomic canonical move.** Create the domain rails, move each
   rule body according to Section 3, update every registry `constitution_ref`,
   root catalogue, pack index, internal link, context-manifest reference, and
   gate mapping in the same reviewed change. Replace root 01–09 content with
   redirect shells. Validate every old and new path/anchor in that commit.
6. **Wire domain-aware enforcement.** Gate registry entries declare their owning
   `ci` automation concern and governed rule IDs; registry/checker tests prove
   registry-to-rule and rule-to-gate links. A rule retains its owning domain even
   where a CI script enforces it.
7. **Migrate factory guidance.** Update
   `.agents/skills/engram-factory/references/context.md` so each stage reads the
   root charter plus every pack selected by its actual blast radius; planners
   read the central component/gate registries; reviewers use the same canonical
   catalogue. Artifact frontmatter continues to carry rule IDs, never paths.
8. **Prove and remove transition mode.** Run all constitution and artifact tests,
   the existing rule-class gates, and mutation tests for a missing reciprocal,
   an orphan rail, a stale redirect, and an unknown artifact rule ID. Once all
   in-repository callers use canonical references, retain redirects for stable
   inbound compatibility but remove flat-layout discovery fallback.

The transition never relies on an active deviation. The checker expands its
accepted topology before canonical prose moves; the move changes all linked
surfaces atomically; redirects preserve historical links; and gates remain
blocking throughout.

## 7. Self-hosting acceptance test

**Fresh webapp repository.** Install the root charter plus the five empty pack
indexes and central registry schemas. Instantiate only relevant rules: product
owns browser/API compatibility, architecture owns service/data boundaries,
build owns reproducible bundle/release rules, testing owns fixture and
acceptance-test truth, and CI owns automation/evidence. Register each new rule
once, add a gate mapping, then demonstrate a nested rule cited in a job artifact
and a deliberate orphan reference rejected by the recursive checker. No Engram
engine-neutrality, N-API, Cargo, MCP, or launcher assumption is required.

**Engram dogfood path.** Apply the mapping in Section 3 to the current EN/SP/CI/
DD/PC/CE/ZD/OR/PR IDs and annotate existing five registries with their domains.
`tools/ci/check_constitution.py` is itself a declared component and `GATE-*`
entry, is linked from the `ci` pack, and validates the root and its own pack.
`check_job_artifacts.py` proves that factory artifacts resolve a rule located in
any pack. Add fixtures that mutate an EN link, a CI gate back-reference, and a
legacy redirect; each must fail. This is self-hosting without special exemption:
the constitution change supplies its own reciprocal references, gate mapping,
and evidence before it can claim conformance.

## Evidence used and constraints retained

This design is grounded in the current root index and rails 01–09, the five
central registries, `check_constitution.py`, `check_job_artifacts.py`, and the
factory context manifest. In particular, the existing constitution checker only
globs root `0[1-9]-*.md`, and the artifact gate currently scans root `0*.md` for
rule IDs; both are concrete migration seams. Current central registries already
model canonical implementation ownership, events, gates, distributions, and
components, so centralizing their identities avoids a new reconciliation class.
Index mining returned **index empty for federated constitution domain packs**
and **index empty for constitution gates registries migration** (both recall
queries returned `hit_count: 0`); no index result was fabricated or relied on.

No claim is made that current rails' stated planned/existing status is accurate
beyond those inspected files. This document specifies a target topology and
migration contract, not an adoption decision or a live conformance assertion.
