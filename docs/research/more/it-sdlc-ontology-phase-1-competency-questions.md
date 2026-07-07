---
type: ontology-phase
title: Phase 1 - Competency Questions
slug: it-sdlc-ontology-phase-1-competency-questions
project: unified-knowledge-base
phase: 1
status: drafted
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-27
tags:
  - tiaa
  - unified-knowledge-base
  - ontology
  - competency-questions
---

## Purpose

Competency questions define what the ontology must be able to answer. They are the acceptance criteria for the semantic model, source mappings, validation rules, and graph queries.

This initial set is organized around the pilot goal: use the application landscape to support solution design while preserving traceability across enterprise architecture, SDLC, integration, operations, risk, and compliance.

## Application Purpose and Capability

1. What does this product, digital experience, or software system do?
2. Which business capabilities, processes, journeys, or outcomes does it support?
3. Which user groups, channels, or consuming systems use it?
4. Is it a system of record, system of engagement, system of insight, system of automation, or supporting platform?
5. What business events does it create, consume, or respond to?

## Application Landscape and Solution Design

6. For a proposed solution, which existing products, digital experiences, software systems, deployable units, or interface contracts could be reused?
7. What required attributes must a target software system, deployable unit, or interface contract have to participate in a solution design?
8. Which systems meet required attributes such as criticality, availability, data classification, region, latency, volume, hosting model, security posture, and compliance posture?
9. Which systems are approved, deprecated, strategic, tactical, restricted, or not recommended for new solution designs?
10. What architecture standards, design patterns, or guardrails apply to this system or domain?
11. Which systems, deployable units, or interface contracts are candidates for replacement, consolidation, or modernization?
12. Which constraints should be considered before proposing a new build or integration?

## Integration Surfaces

13. What interface contracts does this system or deployable unit expose?
14. What APIs, services, queues, topics, streams, files, batch feeds, UI fragments, agent capabilities, or SaaS connectors does it consume?
15. What integration types are used: REST, GraphQL, event, stream, batch, file, ETL, message queue, SaaS connector, database replication, or direct database access?
16. What are the data contracts, schemas, payloads, protocols, authentication methods, rate limits, and SLAs for each integration?
17. Which systems, deployable units, or interface contracts are upstream and downstream of this target?
18. What would be impacted if an integration changed, degraded, or failed?
19. Which integrations are strategic, tactical, legacy, restricted, or pending retirement?

## Data and Information

20. What data domains and entities does the system or deployable unit create, read, update, or delete?
21. Which data is mastered here versus copied, cached, derived, or consumed?
22. What sensitive, regulated, confidential, or restricted data does it process?
23. What reports, analytics, AI/RAG use cases, or downstream decisions depend on its data?
24. What data quality, retention, lineage, residency, and access requirements apply?

## SDLC and Delivery

25. Which repositories, deployable units, pipelines, artifacts, builds, releases, and deployments belong to this software system?
26. Which requirements, epics, stories, defects, and change requests are associated with it?
27. What changed between two releases, and what capabilities, APIs, controls, consumers, or environments were affected?
28. Which environments does it run in, and how do deployments move through those environments?
29. Which deployable units participate in a cross-system, cross-unit, or cross-experience feature or solution flow?

## Operations, Risk, and Compliance

30. Who owns the product, digital experience, software system, deployable unit, interface contract, data, controls, integrations, and support model?
31. Which incidents, problems, changes, known errors, and post-incident reviews are linked to it?
32. Which controls apply, and what evidence proves compliance?
33. Which risks, exceptions, unsupported technologies, or architectural debts exist?
34. What are its SLAs/SLOs, support model, observability coverage, backup requirements, and recovery requirements?
35. Which dependencies create concentration risk, operational fragility, or compliance exposure?

## Phase 1 Deliverable

The reviewed output of this phase is a prioritized competency-question set grouped into:

- Application purpose.
- Solution design attributes.
- Integration surfaces.
- Data responsibilities.
- SDLC traceability.
- Operations.
- Risk and compliance.

Each approved competency question should eventually map to:

- Required ontology classes and relationships.
- Required attributes and controlled values.
- Source systems and source fields.
- Validation rules.
- Example graph queries.
- Example product, software-system, deployable-unit, and interface-contract records.

## Review Questions

- Which questions are must-have for the first pilot?
- Which questions are later-phase but should remain visible?
- Which required attributes are missing for solution design?
- Which integration details are essential versus nice-to-have?
- Which source systems can provide the data needed to answer these questions?

## Research Anchors for Progressive Disclosure

Use these anchors when a competency question needs a supporting reference or known model.

| Anchor | Use When |
|--------|----------|
| [C4 Model abstractions](https://c4model.com/abstractions) | Explaining questions about system boundaries, deployable/runtime units, and software decomposition. |
| [Backstage System Model](https://backstage.io/docs/features/software-catalog/system-model/) | Explaining catalog questions about ownership, systems, components, APIs, resources, and domains. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining competency questions about HTTP API contracts, discoverability, and consumers. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining competency questions about event/message/channel integrations. |
| [OSLC Specifications](https://open-services.net/specifications/) | Explaining lifecycle traceability questions across requirements, change, quality, and configuration artifacts. |
| [COBIT](https://www.isaca.org/resources/cobit) | Explaining governance, control, risk, and assurance questions. |

Progressive-disclosure rule:

- Start from the question the ontology must answer.
- Bring in the anchor only when the team asks where the concept comes from or how it maps to common practice.
- Keep the competency-question list tool-neutral; source-specific wording belongs in source profiles.
