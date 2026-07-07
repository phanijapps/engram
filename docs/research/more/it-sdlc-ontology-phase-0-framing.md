---
type: ontology-phase
title: Phase 0 - Frame the Ontology
slug: it-sdlc-ontology-phase-0-framing
project: unified-knowledge-base
phase: 0
status: drafted
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-27
tags:
  - tiaa
  - unified-knowledge-base
  - ontology
  - framing
---

## Purpose

Create a shared enterprise IT and SDLC ontology that describes:

> What a product, digital experience, software system, deployable unit, and interface contract are; what they do; what business capabilities they support; how they integrate; how they are built and operated; what risks and controls apply; and how they can be reused or impacted in future solution designs.

The ontology should act as a living semantic layer over the enterprise application landscape. It should not be only a glossary or static inventory. It should support solution design, impact analysis, governance, AI/RAG grounding, delivery traceability, and operational decision-making.

## Pilot Scope

For any critical product, digital experience, or software system, the ontology should show:

- What it does: business purpose, capabilities supported, user journeys, business processes, key functions, and business events.
- Where it sits: owning team, domain, portfolio, lifecycle state, criticality, environments, and strategic posture.
- How it is built: repositories, components, APIs, dependencies, build history, release history, and deployment history.
- How it integrates: inbound and outbound APIs, events, files, batch jobs, queues, streams, SaaS connectors, and data contracts.
- What it needs for solution design: data classification, latency, availability, throughput, volume, hosting model, region, security, compliance, observability, support model, and recovery requirements.
- How it performs operationally: incidents, problems, changes, SLAs/SLOs, runtime dependencies, support ownership, and observability coverage.
- What governs it: risks, controls, evidence, exceptions, architecture standards, technology standards, and design guardrails.

## Pilot Outcome

Given a proposed solution, the knowledge base should help identify:

- Candidate applications, services, APIs, platforms, and data sources that could be reused.
- Required attributes that a target application or service must satisfy.
- Owners, stewards, and accountable teams.
- Existing integration surfaces and constraints.
- Upstream and downstream dependencies.
- Data responsibilities and sensitivity.
- Operational, architectural, risk, and compliance concerns.
- Impacted capabilities, consumers, controls, and environments.

## Working Anchor

The first pilot should use a critical product, digital experience, or software system and trace it across:

1. Business capability and purpose.
2. Product, experience, software-system, and deployable-unit architecture.
3. Integration surfaces.
4. Data responsibilities.
5. SDLC artifacts and delivery flow.
6. Runtime operations.
7. Risk, controls, evidence, and exceptions.
8. Solution design reuse and impact analysis.

## Design Principle

The ontology should be federated:

- A small enterprise core defines stable cross-domain concepts.
- Domain extensions define specialized concepts for architecture, SDLC, integration, operations, data, risk, and compliance.
- Source-system mappings connect the ontology to real tools and repositories.
- Governance keeps the ontology alive without allowing uncontrolled semantic drift.

## Review Questions

- Does this pilot scope reflect the real solution-design decisions teams need to make?
- Are application purpose, integration surfaces, and required design attributes represented strongly enough?
- Which source systems should be included in the first pilot slice?
- Which application or product should be used as the first concrete example?
- Who should review the ontology for architecture, delivery, operations, and controls?

## Research Anchors for Progressive Disclosure

Use these anchors when explaining why the ontology is framed as a living semantic layer rather than a static inventory.

| Anchor | Use When |
|--------|----------|
| [RDF 1.1 Concepts](https://www.w3.org/TR/rdf11-concepts/) | Explaining why graph-style identifiers and relationships are useful for connecting heterogeneous enterprise knowledge. |
| [OWL 2 Overview](https://www.w3.org/TR/owl2-overview/) | Explaining formal ontology semantics, class/relationship meaning, and reasoning. |
| [SHACL](https://www.w3.org/TR/shacl/) | Explaining why the ontology should validate data quality and not merely document terminology. |
| [PROV-O](https://www.w3.org/TR/prov-overview/) | Explaining provenance: who asserted what, from which system, when, and by what process. |
| [DCAT](https://www.w3.org/TR/vocab-dcat-3/) | Explaining catalog-style metadata for datasets, services, and knowledge assets. |

Progressive-disclosure rule:

- Start with the pilot: solution design needs trusted knowledge about products, systems, integrations, data, owners, risks, and controls.
- Bring in RDF/OWL only when discussing formal graph representation.
- Bring in SHACL/PROV-O when discussing validation, trust, lineage, and evidence.
