---
type: project-brief
title: Unified Knowledge Base - Enterprise IT and SDLC Ontology
slug: it-sdlc-ontology-overview
project: unified-knowledge-base
status: active
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - enterprise-ontology
  - application-landscape
  - sdlc
---

## Synopsis

Unified Knowledge Base is the working project for an enterprise IT and SDLC ontology. The ontology is intended to describe what products, digital experiences, software systems, deployable units, and interface contracts do; how they integrate; how they are built and operated; what risks and controls apply; and how the application landscape can support solution design.

The initial pilot uses a critical product, digital experience, or software system as the anchor and traces it from business capability through software-system architecture, deployable units, interface contracts, integration surfaces, software delivery, operations, risk, and compliance.

## Artifact Source Of Truth

The canonical working artifacts for this effort live in the top-level `outputs/` folder for this Codex workspace. Earlier duplicate output folders and the stale document pack have been removed, so new ontology work should continue in the top-level `outputs/` Markdown files.

## Current Working Definition

Create a shared semantic model that lets enterprise teams answer:

> What does this product, digital experience, software system, deployable unit, or interface contract do; how does it fit into the landscape; what can it integrate with; what design attributes are required; who owns it; how is it delivered and operated; and what risks or controls govern it?

## Current Phase Notes

| Phase | Note | Status |
|------|------|--------|
| Phase 0 | [[it-sdlc-ontology-phase-0-framing]] | Drafted for review |
| Phase 1 | [[it-sdlc-ontology-phase-1-competency-questions]] | Drafted for review |
| Phase 2 | [[it-sdlc-ontology-phase-2-source-standards]] | Recommendation accepted |
| Phase 3 | [[it-sdlc-ontology-phase-3-core-concept-model]] | Product-to-runtime stack accepted |
| Phase 4 | [[it-sdlc-ontology-phase-4-relationship-model]] | Drafted for review |
| Phase 5 | [[it-sdlc-ontology-phase-5-context-selection-operations]] | Drafted for review |
| Phase 6 | [[it-sdlc-ontology-phase-6-source-mapping-ingestion]] | Pilot recommendation accepted |
| Phase 7 | [[it-sdlc-ontology-phase-7-validation-rules]] | Gate-aware draft for review |
| Phase 8 | [[it-sdlc-ontology-phase-8-governance-operating-model]] | Drafted for review |
| Phase 9 | [[it-sdlc-ontology-phase-9-consumer-views-solution-design]] | Solution Design Brief View accepted as first pilot consumer view |
| Phase 10 | [[it-sdlc-ontology-phase-10-minimum-viable-specification]] | Drafted for review |
| Phase 11 | [[it-sdlc-ontology-phase-11-worked-pilot-instance]] | Drafted for review |
| Phase 12 | [[it-sdlc-ontology-phase-12-pilot-execution-review]] | Drafted for review |

## Current Phase Architecture

The phase order now follows the lifecycle of a living enterprise ontology:

1. Frame the purpose, pilot anchor, and questions the ontology must answer.
2. Select standards and define the portable semantic model.
3. Define the relationship model that makes traceability possible.
4. Define context-selection policy before source ingestion, so AI grounding is designed intentionally.
5. Map authoritative sources, ingestion patterns, and assertion states.
6. Define gate-aware validation and governance controls.
7. Generate consumer views, minimum viable specifications, worked examples, and pilot execution plans.

The companion roadmap [[it-sdlc-ontology-feature-roadmap]] translates these phases into the full platform-scale capability groupings, implementation features, storage/index infrastructure, UIs, ingestion pipelines, dependencies, and sequencing. The companion decision brief [[it-sdlc-ontology-build-reduction-options]] evaluates a smaller pilot architecture using Git, repo-local manifests, controlled AI-written wiki pages, context attachment metadata, compiled context packets, file-based agent/tool/action manifests, trace-driven update proposals, and rebuildable context indexes before committing to the full platform build. The companion roadmap [[it-sdlc-ontology-reduced-feature-roadmap]] translates that reduced-build option into a concrete pilot feature sequence. The research anchor [[it-sdlc-ontology-context-layer-vendor-patterns-survey]] captures the applied vendor/practitioner patterns behind the roadmap refinements.

## Intended Outcomes

- A governed ontology for enterprise IT, SDLC, integration, operations, and controls.
- A solution-design knowledge base for discovering reusable products, software systems, deployable units, interface contracts, data sources, constraints, and owners.
- A semantic application landscape that supports impact analysis, design reviews, AI/RAG grounding, portfolio rationalization, and delivery traceability.
- Human-consumable views and solution-design packets assembled from governed ontology facts rather than recreated by hand.
- Context-selection policies that determine what AI systems capture, update, retrieve, withhold, and elicit from humans.
- Compiled context packets that make selected context reviewable before AI generation.
- Agent/tool/action context policies that determine what execution surfaces are visible, relevant, allowed, and auditable for a given design task.
- A living knowledge model that can evolve as source systems, standards, and delivery practices change.

## Working Scope

In scope:

- Products, digital experiences, software systems, deployable units, interface contracts, and runtime services.
- Business capabilities, processes, journeys, and outcomes.
- Product/system ownership, lifecycle state, criticality, and domain alignment.
- Integration surfaces including APIs, events, queues, files, batch jobs, streams, and SaaS connectors.
- Required solution-design attributes such as data classification, latency, availability, volume, hosting, security, compliance, and observability.
- SDLC traceability across requirements, work items, repositories, builds, releases, deployments, and environments.
- Operational traceability across incidents, problems, changes, SLAs/SLOs, support model, and runtime dependencies.
- Risk, controls, evidence, exceptions, and architectural standards.

Out of scope for the first pilot:

- Modeling every enterprise process in detail.
- Replacing CMDB, portfolio, GRC, observability, or SDLC tools.
- Building a universal ontology before validating the first product/system slice.
- Automating ontology updates without human review and governance.

## Current Modeling Decision

The ontology uses a hybrid adaptability model:

- The canonical ontology owns shared semantic meaning.
- Source systems own specific records, evidence, and operational state through explicit source-system profiles.
- Enterprise overlays capture client-specific terminology, tollgates, governance, and delivery-method vocabulary.

This keeps the ontology portable across enterprises with different tooling while still allowing concrete integration with ServiceNow, Ardoq, Jira Align, Jira, Azure DevOps, GitHub, GitLab, observability platforms, GRC systems, CMDBs, and internal repositories.

## Current Ingestion Planning

Phase 5 defines context-selection policy: what should be captured, updated, retrieved, withheld, or elicited. Phase 6 then defines source mapping and conceptual progressive ingestion: how knowledge moves from source facts to candidate facts, reviewed facts, authoritative facts, and living feedback without prescribing a physical implementation.
