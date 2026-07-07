---
type: ontology-phase
title: Phase 2 - Source Standards and Reference Models
slug: it-sdlc-ontology-phase-2-source-standards
project: unified-knowledge-base
phase: 2
status: recommendation-accepted
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-27
tags:
  - tiaa
  - unified-knowledge-base
  - ontology
  - standards
  - reference-models
---

## Purpose

Phase 2 selects the standards, frameworks, and reference models that should inform the enterprise IT and SDLC ontology.

The goal is not to adopt one framework wholesale. The goal is to compose a practical semantic spine from the best parts of several models, while keeping TIAA-specific terms, workflows, and source-system mappings explicit.

## Recommended Composition Strategy

Use a federated model:

- Enterprise core ontology: stable concepts shared across domains, such as Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, BusinessCapability, API, DataAsset, Team, Control, Risk, Environment, Deployment, and Incident.
- Domain extensions: specialized terms for architecture, SDLC, integrations, operations, data, risk, compliance, and solution design.
- Source-system mappings: mappings from CMDB, portfolio tools, SDLC tools, repositories, CI/CD, observability, GRC, and architecture repositories.
- Controlled vocabularies: approved values for lifecycle state, criticality, hosting model, integration type, data classification, strategic posture, technology status, and support model.

## Candidate Source Standards

| Source | Use It For | Adopt As |
|--------|------------|----------|
| ArchiMate | Enterprise architecture concepts such as capability, application-layer concepts, technology service, process, data object, and realization/serving relationships. | EA reference and relationship patterns |
| IT4IT | Digital product, IT value streams, portfolio-to-delivery-to-operations concepts, service/product lifecycle, and IT operating model alignment. | IT operating model reference |
| OSLC | Linked-data style SDLC artifacts such as requirements, change requests, quality artifacts, configurations, defects, and lifecycle links. | SDLC artifact interoperability model |
| ITIL / ISO 20000 | Incident, problem, change, service, configuration item, SLA, service owner, and IT service management practices. | ITSM and operations vocabulary |
| COBIT | Governance objectives, controls, risk, assurance, accountability, and management practices for enterprise IT. | Governance and control vocabulary |
| SPDX / CycloneDX | Software components, dependencies, packages, vulnerabilities, licenses, and SBOM evidence. | Software supply chain vocabulary |
| OpenTelemetry | Services, traces, metrics, logs, events, dependencies, and runtime observability evidence. | Runtime telemetry vocabulary |
| OpenAPI / AsyncAPI | API contracts, operations, schemas, protocols, producers, consumers, events, channels, and message payloads. | Integration contract vocabulary |
| CloudEvents | Event metadata, event source, event type, subject, time, and event payload conventions. | Event-interoperability vocabulary |
| TIAA / RKT artifacts | Tollgates, solution design artifacts, design-authority expectations, feature/component mappings, and internal delivery terminology. | Internal operating model vocabulary |

## Practical Adoption Rules

1. Prefer reuse where a standard already has a strong concept.
2. Do not force every internal concept into a standard if it loses business meaning.
3. Keep internal terms when they are part of TIAA governance, delivery, or architecture practice.
4. Use mappings to standards instead of renaming everything.
5. Treat vendor-specific models as source mappings unless they are intentionally adopted as enterprise policy.
6. Keep the core ontology small and move detail into domain extensions.
7. Record why each source standard was adopted, partially mapped, or excluded.

## Tooling Adaptability Decision

The ontology should be adaptable across enterprises with different tooling by separating canonical meaning from source-system representation.

Recommended position:

> The enterprise ontology owns the shared semantic meaning. Source systems and vendor models own specific records, attributes, evidence, and operational state within declared authority boundaries.

This means ServiceNow CSDM, Ardoq, Jira Align, Jira, Azure DevOps, GitHub, GitLab, CMDBs, observability tools, GRC tools, and spreadsheets should normally be modeled as source-system profiles or mappings into the ontology, not as the ontology core itself.

There can be exceptions where an enterprise explicitly adopts a vendor or internal model as policy. Even then, the adopted model should be wrapped as an enterprise profile so another enterprise can replace it without rewriting the core ontology.

## Pressure Test: Vendor Model as Authoritative Semantics

In this option, the ontology is largely shaped by the dominant enterprise tools. For example, ServiceNow CSDM could define service/application semantics, Ardoq could define architecture entities, and Jira Align could define portfolio/delivery semantics.

Strengths:

- Fastest path to usable data when one tool is already the enterprise system of record.
- Lower mapping burden at the beginning.
- Easier adoption by teams already using that tool's vocabulary.
- Better alignment with existing reports, workflows, and governance screens.

Weaknesses:

- Low portability across enterprises with different tools.
- The ontology inherits vendor-specific assumptions, naming, constraints, and gaps.
- Tool migrations become ontology migrations.
- Mergers, acquisitions, federated business units, or multi-client use cases create semantic conflicts.
- It becomes harder to distinguish enterprise meaning from tool implementation detail.
- Solution-design queries become brittle because the questions depend on the current tool model.

This option works best when:

- The scope is narrow.
- A single platform is mandated as the enterprise operating model.
- Speed matters more than portability.
- The ontology is intended mainly as a semantic facade over that platform.

This option fails when:

- The ontology must travel across enterprises.
- Different domains use different tools.
- The enterprise expects tool replacement, consolidation, or multi-tool coexistence.
- The ontology must support solution design across architecture, SDLC, operations, risk, and data sources.

## Pressure Test: Vendor Models as Source Mappings

In this option, the ontology defines tool-neutral concepts, relationships, and required attributes. Each source system maps into those concepts through a source profile.

Example:

- Canonical concept: SoftwareSystem
- ServiceNow profile: Business Application, Application Service, Configuration Item
- Ardoq profile: Application, Component, Interface
- Jira Align profile: Portfolio Epic, Capability, Feature
- GitHub/GitLab profile: Repository, Pull Request, Commit, Workflow Run
- Observability profile: Service, Span, Metric, Alert

Strengths:

- Portable across enterprises and tooling stacks.
- Stable solution-design queries even when tools differ.
- Better support for tool migration and portfolio consolidation.
- Clear separation between semantic meaning, record authority, and source evidence.
- Allows multiple sources to contribute facts about the same concept.
- Better fit for AI/RAG grounding because the meaning layer remains stable.

Weaknesses:

- Requires initial canonical modeling work.
- Requires mapping governance and maintenance.
- Can become too abstract if not pressure-tested against real source data.
- Needs conflict-resolution rules when multiple tools claim authority for the same attribute.
- May feel unfamiliar to teams who think in a vendor-specific model.

This option works best when:

- The ontology must support multiple enterprises, business units, or tooling stacks.
- The enterprise expects tool changes over time.
- Solution design needs to ask stable questions across tools.
- The ontology must connect architecture, delivery, operations, risk, controls, data, and runtime evidence.

This option fails when:

- The canonical model is designed without real tool data.
- Source ownership is unclear.
- The team does not maintain mappings as tools evolve.
- Governance becomes too slow for delivery teams.

## Recommended Hybrid

Use a canonical ontology with source-system profiles.

Decision status: accepted on 2026-06-27.

Separate three kinds of authority:

| Authority Type | Owns | Example |
|----------------|------|---------|
| Semantic authority | What a concept means | The ontology defines Product, SoftwareSystem, DeployableUnit, InterfaceContract, IntegrationSurface, Control, Deployment |
| Record authority | Which system owns specific facts | CMDB owns application lifecycle state; Jira owns story status; CI/CD owns deployment timestamp |
| Policy authority | Which values and rules are allowed | Architecture governance owns approved hosting models and strategic posture values |

This lets the ontology remain reusable while still respecting operational systems of record.

## Adaptability Pattern

The ontology should be packaged in layers:

1. Core ontology: tool-neutral enterprise concepts.
2. Domain extensions: architecture, SDLC, integration, operations, risk, compliance, and data.
3. Controlled vocabularies: lifecycle state, criticality, data classification, integration type, hosting model, support model, and strategic posture.
4. Source profiles: mappings for ServiceNow, Ardoq, Jira Align, Jira, Azure DevOps, GitHub, GitLab, observability, GRC, and other tools.
5. Enterprise overlay: client-specific terms, tollgates, governance artifacts, and delivery-method terminology.

With this pattern, a new enterprise can replace source profiles and the enterprise overlay while reusing the core ontology and domain extensions.

## Proposed Semantic Layers

### 1. Business and Portfolio Layer

Primary concepts:

- BusinessCapability
- BusinessProcess
- ValueStream
- Journey
- Product
- DigitalExperience
- Portfolio
- Outcome
- Stakeholder

Likely sources:

- ArchiMate
- IT4IT
- Internal portfolio and RKT operating-model artifacts

### 2. Product-to-Runtime Landscape Layer

Primary concepts:

- SoftwareSystem
- DeployableUnit
- InterfaceContract
- RuntimeService
- Platform
- TechnologyComponent
- LifecycleState
- StrategicPosture
- Criticality
- Owner

Likely sources:

- ArchiMate
- CMDB/application portfolio model
- Internal architecture standards

### 3. Integration and Data Layer

Primary concepts:

- API
- APIEndpoint
- Event
- Topic
- Queue
- BatchFeed
- FileTransfer
- DataContract
- Schema
- DataEntity
- DataAsset
- DataClassification
- Producer
- Consumer

Likely sources:

- OpenAPI
- AsyncAPI
- CloudEvents
- ArchiMate
- Internal integration standards

### 4. SDLC and Delivery Layer

Primary concepts:

- Requirement
- Epic
- Feature
- Story
- WorkItem
- Repository
- Commit
- PullRequest
- Build
- Artifact
- Release
- Deployment
- Environment
- TestCase
- Defect

Likely sources:

- OSLC
- IT4IT
- Internal SDLC and RKT artifacts
- Jira / Jira Align / GitHub / GitLab / CI/CD source models

### 5. Operations and Runtime Layer

Primary concepts:

- Service
- RuntimeService
- Incident
- Problem
- Change
- KnownError
- SLA
- SLO
- Metric
- Trace
- Log
- Alert
- Dependency

Likely sources:

- ITIL / ISO 20000
- OpenTelemetry
- CMDB
- Observability tools

### 6. Risk, Control, and Compliance Layer

Primary concepts:

- Risk
- Control
- ControlObjective
- Evidence
- Exception
- Policy
- Standard
- Finding
- Remediation
- AssuranceActivity

Likely sources:

- COBIT
- GRC tools
- Internal policy and control libraries
- SPDX / CycloneDX for supply-chain evidence

## Phase 2 Deliverables

The reviewed output of this phase should be:

- A source-standard inventory.
- A decision for each source: adopt, map, reference, or exclude.
- A namespace and naming strategy.
- A source-of-truth map for each concept family.
- Initial controlled vocabularies for solution-design attributes.
- A short list of internal artifacts that must be represented.

## Review Questions

- Which standards are mandatory versus just useful references?
- Which TIAA internal artifacts should be treated as authoritative?
- Which source systems are the best systems of record for product, software-system, integration, SDLC, operations, and control data?
- Which solution-design attributes need controlled vocabularies first?
- Should ServiceNow CSDM, Ardoq, Jira Align, or other vendor models be treated as enterprise semantics or only source mappings?

## Research Anchors for Progressive Disclosure

Use these anchors when explaining why a source standard is adopted, mapped, referenced, or excluded.

| Anchor | Use When |
|--------|----------|
| [ArchiMate](https://publications.opengroup.org/standards/archimate) | Explaining enterprise architecture concepts and cross-layer relationships. |
| [IT4IT](https://publications.opengroup.org/standards/it4it) | Explaining IT operating model, product/value-stream, and digital delivery concepts. |
| [OSLC Specifications](https://open-services.net/specifications/) | Explaining linked lifecycle artifact interoperability across requirements, change, quality, and configuration. |
| [COBIT](https://www.isaca.org/resources/cobit) | Explaining governance, control, risk, and assurance vocabulary. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining HTTP API contract source models. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining event/message/channel contract source models. |
| [OpenTelemetry](https://opentelemetry.io/docs/concepts/) | Explaining runtime telemetry concepts such as traces, metrics, logs, and observable services. |
| [SPDX](https://spdx.dev/) and [CycloneDX](https://cyclonedx.org/) | Explaining software supply-chain, dependency, package, license, vulnerability, and SBOM evidence. |

Progressive-disclosure rule:

- Start with the adopted hybrid: canonical ontology owns meaning; source systems own records/evidence.
- Bring in standards only to explain a concept family or mapping decision.
- Treat vendor models as source profiles unless the enterprise explicitly adopts them as policy.
