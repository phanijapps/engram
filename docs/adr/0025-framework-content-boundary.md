# ADR-0025: Framework/content boundary: ship mechanism, not domain ontology content

- **Status:** Proposed
- **Date:** 2026-07-13
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0013 (context-graph packets), `docs/research/engram-framing-synthesis.md` (product/bespoke line), ADR-0001 (workspace boundaries), ADR-0022 (engine neutrality — analogous "keep X out of neutral layers" lint)

## Decision summary

- **Decision:** Engram ships the framework *mechanism* (types + composition + belief/reconciliation engine) in core; domain taxonomy/ontology *content* is consumer-loaded and never lives in `core/domain`.
- **Because:** Engram is a general framework — baking in banking/IT/healthcare vocabulary would make it a one-off deliverable; the framing synthesis flags this as the boundary most likely to be violated under delivery pressure.
- **Applies to:** what constitutes "framework" vs "content" across `core/domain`, `core/retrieval`, `core/knowledge`, and any adapter/on-top layer.
- **Tradeoff accepted:** no domain value out-of-the-box — a consumer must supply (or load) an ontology + taxonomy before packets are non-trivial.
- **Revisit if:** a consumer need cannot be met without core content (e.g., a universally-required `EntityKind`), or delivery pressure pushes domain content into core to unblock a pilot.

## Context

Engram models a context graph — knowledge graph + ontology + taxonomy + belief + policy + bi-temporal validity. The open question (RFC-0013 Q1): does Engram ship a *reference domain vocabulary* in core (a spine of capability/domain/segment/service classes), or only the *mechanism* for defining one?

Forces:

- **Generality.** Banking, IT-SDLC, and healthcare each need different vocabularies; baking one into core picks a domain winner and bakes a client engagement's ontology (`docs/research/engram-framing-synthesis.md` — the IT-SDLC spine is one consumer's) into a general framework.
- **Boundary durability.** The framing synthesis draws the product/bespoke line — *compiled context packet + trace + reconciliation = product; the domain vocabulary = bespoke per consumer* — and warns that blurring it is "the decision most likely to be violated under delivery pressure" (it should be an ADR).
- **Author intent.** `docs/about.md:26` — Engram is the layer for "Knowledge Graph, Context Graph, anything memory": a framework, not a domain product.
- **Contract stability.** Core types are frozen-ish (v1 contract surface); consumer vocabularies should evolve freely under `TaxonomyProposal` governance without forcing core contract changes.

## Decision

> Engram ships the framework **mechanism** — the domain types (`ApplicabilityRule`, `ContextSubgraph`, `DecisionTrace`), the composition machinery, the belief/reconciliation engine, and the existing `Ontology`/`OntologyClass`/`Property`/`Axiom` + `ConceptScheme`/`Concept` types that let any vocabulary be defined — but ships **no domain ontology content**: no reference spine vocabulary, no banking/IT/healthcare classes. A consumer registers its own `Ontology` + `ConceptScheme` sets and its entity→class/concept mappings. A starter spine may ship as a consumer example under `examples/`, never in `core/domain`.

**Enum grey zone (the one place "content" touches a core enum).** Additive `EntityKind` values are core changes. Rule: only *truly generic* kinds belong in `core/domain` (the existing set: `endpoint`, `function`, `method`, `module`, `class`, `struct`, `trait`, `concept`, …). Domain-shaped kinds (`capability`, `domain`, `segment`) are **not** added to the core enum; consumers type those via `ontologyClassRefs` (RFC-0013 D3) + `kind=concept` where no generic kind fits. A future proposal to add a domain kind must show it is generic across consumers, not a single domain's need.

## Decision drivers

- **Generality / framework reuse** — Engram must serve multiple domains without picking one.
- **Out-of-the-box value** — a shipped spine makes the first consumer faster (the competing force).
- **Boundary durability** — the synthesis's explicit warning that this line erodes under delivery pressure.
- **Contract stability** — consumer content must be free to evolve without core contract churn.

## Consequences

**Positive:**
- Engram stays a general framework; banking/IT/healthcare are peer consumers, not the product boundary.
- Consumer vocabularies evolve freely (`TaxonomyProposal` governance) with no core contract change.
- The belief/reconciliation value proposition is domain-agnostic.

**Negative:**
- No domain value out-of-the-box — a consumer must supply (or load) an ontology + taxonomy before packets are non-trivial. Mitigation: a starter spine ships as a consumer example under `examples/`.
- A grey zone remains on additive `EntityKind` values; resolved by the "generic kinds only" rule above (enforced by review, see Confirmation).
- Reviewer burden: someone must hold the line against "just this one domain class in core" creep during delivery.

**Revisit if:** a consumer need cannot be met without core content (e.g., a universally-required `EntityKind` across all consumers), or delivery pressure pushes domain content into core to unblock a pilot.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** `core/domain` (and `core/retrieval`, `core/knowledge`) contains only generic mechanism types; domain-specific classes, concept schemes, or spine vocabularies live under `examples/` or consumer packages, not core. New `EntityKind` additions cite their cross-domain generality.
- **Owner:** phanijapps (architecture). A grep-style lint mirroring `check-engine-neutrality.sh` could flag domain terms (e.g. `banking`, `account`, `kyc`) as class/scheme identifiers under `core/` if erosion is observed; not mechanized yet by intent.

## Alternatives considered

- **Ship a minimal reference spine in `core/domain`** (the synthesis's ~8 concepts, or a capability/domain/segment/service/endpoint vocabulary). Rejected against *generality* and *boundary durability*: it picks a domain winner, freezes a client engagement's ontology into the framework, and is the exact erosion the synthesis warns about. (This was the option an early draft of RFC-0013 quietly adopted; the adversarial review caught it.)
- **Ship no mechanism either — pure host-side composition.** Rejected: the contract types (`ApplicabilityRule`, `ContextSubgraph`, `DecisionTrace`, the ontology-class link) are portable framework truth that belongs in `core/domain`, not host-specific behavior. Removing them would lose engine neutrality (ADR-0022) and the seam discipline (ADR-0009).
- **Ship the spine as a loadable consumer ontology under `examples/`.** Accepted — as the companion to this decision, not an alternative: a starter spine is consumer content, delivered as an example.

## References

- [RFC-0013](../rfcs/0013-context-graph-packets.md) — context-graph packets; Q1 (spine scope), gated by this ADR.
- `docs/research/engram-framing-synthesis.md` — the product/bespoke line and the boundary-erosion warning.
- `docs/about.md:26` — author intent ("Knowledge Graph, Context Graph, anything memory").
- [ADR-0001](0001-workspace-boundaries.md) — workspace boundaries (a parallel "keep concerns separated" decision).
- [ADR-0022](0022-engine-grid-vs-backend-recipe.md) — engine neutrality; the `check-engine-neutrality.sh` lint pattern this boundary's confirmation mirrors.
