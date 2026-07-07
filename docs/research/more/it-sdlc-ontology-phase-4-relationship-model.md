---
type: ontology-phase
title: Phase 4 - Relationship Model
slug: it-sdlc-ontology-phase-4-relationship-model
project: unified-knowledge-base
phase: 4
status: drafted
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-27
tags:
  - tiaa
  - unified-knowledge-base
  - ontology
  - relationships
---

## Purpose

Phase 4 defines the first-pass relationship model for the enterprise IT and SDLC ontology.

The relationship model is the verb layer. It explains how products, experiences, systems, deployable units, interface contracts, data, SDLC artifacts, runtime facts, risks, controls, evidence, and owners connect.

This phase should make the ontology queryable for solution design:

> Given a proposed solution, what products, experiences, software systems, deployable units, interfaces, data, dependencies, owners, controls, and operational constraints are relevant?

## Modeling Rules

Relationships should be:

- Human-readable: use verbs that architects, engineers, product owners, and operators can understand.
- Directional: make source and target direction explicit.
- Queryable: every relationship should support at least one competency question.
- Tool-neutral: source systems may use different relationship names, but the ontology should normalize them.
- Evidence-aware: important relationships should record source, confidence, timestamp, and authority.
- Scoped: avoid vague "related to" edges except as temporary ingestion hints.

## Relationship Families

| Family | Purpose | Example Relationships |
|--------|---------|----------------------|
| Value and purpose | Connect business intent to products and systems. | supports, enables, realizes, contributesTo |
| Composition | Show how things are assembled. | composedOf, partOf, includes, contains |
| Exposure and consumption | Show how capabilities are offered and used. | exposes, consumes, implements, invokes |
| Integration and data flow | Show technical interaction and exchanged data. | produces, subscribesTo, sendsTo, receivesFrom, governedBy |
| Delivery traceability | Connect requirements to code, builds, releases, and deployments. | implements, changes, builtFrom, produces, packagedIn, deployedBy |
| Runtime and operations | Connect deployed things to environments, telemetry, incidents, and support. | runsIn, observedAs, affects, dependsOn, supportedBy |
| Governance and compliance | Connect systems to standards, regulations, policies, NFRs, controls, evidence, and exceptions. | constrainedBy, derivedFrom, appliesWhen, appliesTo, satisfies, mitigates, violates |
| Ownership and accountability | Connect people, teams, and organizations to managed objects. | owns, stewards, supports, accountableFor, approves |

## Core Relationship Set

### Business and Product Relationships

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| supportsCapability | Product, SoftwareSystem | BusinessCapability | The source helps deliver or enable the capability. |
| enablesProcess | SoftwareSystem, DigitalExperience | BusinessProcess | The source enables execution of the process. |
| realizesOutcome | Product | Outcome | The product is intended to produce or influence the outcome. |
| createsBusinessEvent | SoftwareSystem, DeployableUnit | BusinessEvent | The source emits or causes the business event. |
| respondsToBusinessEvent | SoftwareSystem, DeployableUnit | BusinessEvent | The source reacts to the business event. |

### Product-to-Runtime Composition

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| experiencedThrough | Product | DigitalExperience | The product is consumed through the experience. |
| realizedBy | Product | SoftwareSystem | The product depends on the system to deliver value. |
| composedFrom | DigitalExperience | DeployableUnit | The experience includes or assembles the deployable unit. |
| composedOf | SoftwareSystem | DeployableUnit | The system is implemented through one or more deployable units. |
| deployedAs | DeployableUnit | RuntimeService | The deployable unit appears as this running workload or service. |
| runsIn | RuntimeService, Deployment | Environment | The runtime service or deployment exists in the environment. |

### Interface and Integration Relationships

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| exposesContract | SoftwareSystem, DeployableUnit | InterfaceContract | The source provides the contract for consumers. |
| consumesContract | SoftwareSystem, DeployableUnit | InterfaceContract | The source depends on the contract as a consumer. |
| implementsContract | DeployableUnit | InterfaceContract | The deployable unit implements the contract. |
| realizesSurface | InterfaceContract | IntegrationSurface | The abstract contract is realized by an API, event, file, queue, stream, or UI fragment. |
| exchangesData | InterfaceContract, IntegrationSurface | DataEntity | The interface exchanges the data entity. |
| governedByContract | IntegrationSurface | DataContract | The integration surface is governed by the data contract. |
| upstreamOf | SoftwareSystem, DeployableUnit, InterfaceContract | SoftwareSystem, DeployableUnit, InterfaceContract | The source provides data or behavior used by the target. |
| downstreamOf | SoftwareSystem, DeployableUnit, InterfaceContract | SoftwareSystem, DeployableUnit, InterfaceContract | Inverse of upstreamOf. |

### Data Responsibility Relationships

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| mastersData | SoftwareSystem | DataEntity | The system is the authoritative source for the data entity. |
| readsData | SoftwareSystem, DeployableUnit | DataEntity | The source reads the data entity. |
| writesData | SoftwareSystem, DeployableUnit | DataEntity | The source writes or updates the data entity. |
| derivesData | DataAsset | DataEntity, DataAsset | The data asset is derived from the target. |
| classifiedAs | DataEntity, DataAsset | DataClassification | The data has this classification. |

### SDLC and Delivery Relationships

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| specifiedBy | Product, SoftwareSystem, DeployableUnit | Requirement | The target expresses a need for the source. |
| implementedBy | Requirement | WorkItem | The work item implements the requirement. |
| changes | WorkItem, Change | SoftwareSystem, DeployableUnit, InterfaceContract, DataContract | The source changes the target. |
| implementedIn | DeployableUnit | Repository | The deployable unit's implementation is stored in the repository. |
| builtFrom | Build | Repository | The build used source from the repository. |
| producesArtifact | Build | Artifact | The build produced the artifact. |
| includedInRelease | Artifact, WorkItem, Change | Release | The source is included in the release. |
| deployedBy | Release, Artifact | Deployment | The release or artifact is deployed by the deployment. |
| deployedTo | Deployment | Environment | The deployment targets the environment. |

### Operations and Runtime Relationships

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| dependsOn | SoftwareSystem, DeployableUnit, RuntimeService | SoftwareSystem, DeployableUnit, RuntimeService, Platform, InterfaceContract | The source needs the target to work. |
| observedAs | RuntimeService | Metric, Trace, Log, Alert | The runtime service is observed through telemetry. |
| affects | Incident, Problem, Risk | Product, DigitalExperience, SoftwareSystem, DeployableUnit, RuntimeService, InterfaceContract | The source affects the target. |
| causedBy | Incident | Deployment, Change, RuntimeService, InterfaceContract | The incident is caused by or linked to the target. |
| supportedBy | Product, SoftwareSystem, DeployableUnit, RuntimeService | Team | The team provides operational support. |
| governedBySLO | RuntimeService, InterfaceContract | SLO | The source is measured by the SLO. |

### Governance, Risk, and Compliance Relationships

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| constrainedBy | Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, DataEntity, RuntimeService | Standard, Policy, Regulation, Constraint, QualityAttributeRequirement | The source must comply with or be shaped by the target. |
| derivedFrom | Constraint, Control, Requirement, QualityAttributeRequirement | NormativeSource, Standard, Policy, Regulation | The source is derived from an authoritative normative source. |
| definesConstraint | NormativeSource, Standard, Policy, Regulation | Constraint | The source defines or implies the constraint. |
| definesControl | NormativeSource, Standard, Policy, Regulation | Control | The source defines or requires the control. |
| definesRequirement | NormativeSource, Standard, Policy, Regulation | Requirement, QualityAttributeRequirement | The source defines or requires the requirement. |
| scopedBy | Standard, Policy, Regulation, Constraint, Control, QualityAttributeRequirement | ApplicabilityRule | The target determines when the source applies. |
| appliesWhen | ApplicabilityRule | Attribute condition, relationship condition, lifecycle state, gate, or target class | The rule applies under this condition. |
| appliesTo | Control | Product, SoftwareSystem, DeployableUnit, DataEntity, InterfaceContract | The control applies to the target. |
| requirementAppliesTo | Requirement, QualityAttributeRequirement | Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, RuntimeService | The requirement applies to the target. |
| satisfiedBy | Control | Evidence | Evidence proves or supports satisfaction of the control. |
| requirementSatisfiedBy | Requirement, QualityAttributeRequirement | Evidence, TestCase, Metric, SLO, ArchitectureDecision | Evidence proves or supports satisfaction of the requirement. |
| mitigates | Control, Remediation | Risk | The source reduces the risk. |
| violates | Finding, Exception | Standard, Policy, Regulation, Constraint, Control, Requirement, QualityAttributeRequirement | The source records non-compliance or approved deviation. |
| exceptedBy | Constraint, Control, Requirement, QualityAttributeRequirement, Standard, Policy | Exception | The source has an approved or requested exception. |
| approvedBy | Exception, Change, Release, ArchitectureDecision | Person, Team, Organization | The target approved the source. |

### Normative Applicability Path

```text
NormativeSource / Regulation / Policy / Standard
  definesConstraint Constraint
  definesRequirement QualityAttributeRequirement
  definesControl Control
  scopedBy ApplicabilityRule
  appliesWhen target attributes or relationships match
  constrainedBy / appliesTo / requirementAppliesTo target
  satisfiedBy / requirementSatisfiedBy Evidence
  mayHave Exception
```

Example:

```text
Restricted Data Handling Policy
  definesControl EncryptionAtRestControl
  definesConstraint NoPublicExposureConstraint
  scopedBy RestrictedDataApplicabilityRule

RestrictedDataApplicabilityRule appliesWhen:
  InterfaceContract exchangesData DataEntity classifiedAs Restricted

EncryptionAtRestControl appliesTo participant-profile API
NoPublicExposureConstraint constrainedBy participant-profile API
EncryptionAtRestControl satisfiedBy encryption-config-evidence
```

### Ownership and Accountability Relationships

| Relationship | From | To | Meaning |
|--------------|------|----|---------|
| owns | Team, Organization, Person | Product, SoftwareSystem, DeployableUnit, InterfaceContract, DataEntity, Control | The source owns the target. |
| stewards | Team, Person | DataEntity, DataAsset, Standard, OntologyConcept | The source manages semantic or data quality. |
| accountableFor | Team, Person, Organization | Product, SoftwareSystem, Risk, Control, Outcome | The source is accountable for the target. |
| memberOf | Person | Team, Organization | The person belongs to the team or organization. |
| provides | Organization, Platform, SoftwareSystem | Platform, SoftwareSystem, InterfaceContract, ServiceOffering | The source provides the target. |

## Cardinality Guidance

These are not hard global rules yet; they are design-time expectations to pressure-test during pilot mapping.

| Pattern | Expected Cardinality | Notes |
|---------|----------------------|-------|
| Product experiencedThrough DigitalExperience | 0..N | Some products may be internal or non-digital at first. |
| Product realizedBy SoftwareSystem | 1..N | A product should usually be realized by one or more systems. |
| DigitalExperience composedFrom DeployableUnit | 1..N | A digital experience should be traceable to deployable units or external systems. |
| SoftwareSystem composedOf DeployableUnit | 1..N | If no deployable unit is known, mark the composition incomplete. |
| DeployableUnit implementedIn Repository | 0..N | SaaS, vendor, or COTS units may not have internal repositories. |
| DeployableUnit exposesContract InterfaceContract | 0..N | Some units are internal workers with no direct consumer-facing contract. |
| InterfaceContract realizedSurface IntegrationSurface | 1..N | A contract should have at least one concrete realization. |
| Deployment deployedTo Environment | 1 | A deployment should target one environment. |
| RuntimeService runsIn Environment | 1..N | DR and active-active patterns may span environments/regions. |
| Control appliesTo target | 1..N | Controls without scope are not actionable. |
| Control satisfiedBy Evidence | 0..N | Missing evidence should be visible as a gap. |
| Product/SoftwareSystem/DeployableUnit owns Team | 1..N | At least one owner should exist for governed assets. |

## Relationship Evidence Model

Important relationships should carry metadata:

| Field | Meaning |
|-------|---------|
| sourceSystem | System that asserted the relationship. |
| sourceRecordId | Source record identifier. |
| assertedBy | Person, team, connector, or automation that asserted it. |
| assertedAt | Timestamp. |
| confidence | Confidence score or category. |
| authorityLevel | authoritative, inferred, proposed, imported, or deprecated. |
| validFrom / validTo | Time window where the relationship is valid. |
| reviewStatus | draft, reviewed, approved, disputed, deprecated. |

This matters because relationship quality will vary. A dependency discovered from runtime telemetry is useful, but it is not the same kind of truth as an architect-approved interface contract or a CMDB-owned support relationship.

## Solution-Design Traceability Paths

These paths should become common graph queries.

### Product Impact Path

```text
Product
  experiencedThrough DigitalExperience
  composedFrom DeployableUnit
  exposes/consumes InterfaceContract
  exchangesData DataEntity
  constrainedBy Standard/Policy
  requirementAppliesTo QualityAttributeRequirement
  affectedBy Incident/Risk/Change
```

### Application Landscape Reuse Path

```text
BusinessCapability
  supportedBy Product/SoftwareSystem
  exposesContract InterfaceContract
  governedBy DataContract
  ownedBy Team
  constrainedBy Standard
```

### Delivery-to-Operations Path

```text
Requirement
  implementedBy WorkItem
  changes DeployableUnit
  builtFrom Repository
  producesArtifact Build
  includedIn Release
  deployedBy Deployment
  deployedAs RuntimeService
  affectedBy Incident
```

### Control Evidence Path

```text
Control
  appliesTo SoftwareSystem/DeployableUnit/InterfaceContract/DataEntity
  satisfiedBy Evidence
  producedBy Build/Deployment/Test/Review
  approvedBy Person/Team
```

### Standards and NFR Applicability Path

```text
Standard / Policy / Regulation
  derivedInto Constraint / Control / QualityAttributeRequirement
  scopedBy ApplicabilityRule
  appliesTo target based on target attributes
  satisfiedBy planned or actual Evidence
  exceptedBy approved Exception
```

## Relationships to Avoid or Quarantine

Avoid making these first-class unless there is no better alternative:

- relatedTo
- associatedWith
- linkedTo
- impacts, without direction and impact type
- owns, when the relationship really means supports, stewards, funds, approves, or operates
- dependsOn, when the relationship is actually consumesContract, runsOn, hostedBy, readsData, or calls

If broad relationships are ingested from tools, quarantine them as provisional edges until they can be normalized.

## Phase 4 Deliverables

The reviewed output of this phase should be:

- Approved relationship families.
- Approved core relationship names.
- Directionality for each relationship.
- Initial cardinality guidance.
- Evidence metadata for relationship assertions.
- Relationship paths that answer the main competency questions.
- A list of vague or source-specific relationships to quarantine.

## Review Questions

- Are the relationship names understandable to humans?
- Do the relationships preserve the Product -> DigitalExperience -> SoftwareSystem -> DeployableUnit -> InterfaceContract -> RuntimeService distinctions?
- Are upstream/downstream and dependsOn too vague, or useful enough if constrained?
- Which relationships must carry source authority and review status?
- Which relationships should be mandatory for the first pilot?
- Are any relationships too detailed for the core and better moved to a domain extension?

## Research Anchors for Progressive Disclosure

Use these references only when needed to explain a modeling choice:

| Anchor | Use When |
|--------|----------|
| [C4 Model abstractions](https://c4model.com/abstractions) | Explaining composition from system to deployable/runtime boundaries and internal components. |
| [Backstage System Model](https://backstage.io/docs/features/software-catalog/system-model/) | Explaining system/component/API/resource catalog relationships. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining why HTTP APIs should be modeled as InterfaceContracts with machine-readable definitions. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining event/message/channel relationships and asynchronous integration contracts. |
| [Micro Frontends](https://micro-frontends.org/) | Explaining digital-experience composition from independently owned frontend fragments. |
| [Backends for Frontends pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/backends-for-frontends) | Explaining relationship patterns between digital experiences, BFF deployable units, and backend systems. |
