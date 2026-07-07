---
type: ontology-phase
title: Phase 7 - Validation Rules
slug: it-sdlc-ontology-phase-7-validation-rules
project: unified-knowledge-base
phase: 7
status: gate-aware-draft
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - ontology
  - validation
  - shacl
---

## Purpose

Phase 7 defines the validation rules that make the ontology operational.

The ontology should not only describe the enterprise landscape. It should identify missing ownership, incomplete integration contracts, weak solution-design evidence, unsupported technology, control gaps, and traceability breaks.

The validation layer should answer:

> Is this product, digital experience, software system, deployable unit, interface contract, data responsibility, deployment, or control record complete enough to support solution design, governance, delivery, and operations?

## Validation Philosophy

Use validation as guidance before using it as enforcement.

Rules should be classified by severity:

| Severity | Meaning | Action |
|----------|---------|--------|
| Info | Useful enrichment or recommended metadata is missing. | Show as improvement opportunity. |
| Warning | Important design or governance information is incomplete. | Allow progression with review. |
| Error | Required data is missing or structurally invalid. | Block promotion to approved state. |
| Critical | Control, security, compliance, or operational-risk requirement is violated. | Block and require exception or remediation. |

Rules should be scoped by both lifecycle and governance gate. A rule that is blocking at deploy-to-production may be advisory at solution design, because the solution design gate often approves intent, architecture, required attributes, and risk posture before implementation evidence exists.

Lifecycle states:

- Draft: minimum identity and ownership.
- Candidate: enough attributes for review.
- Approved: complete enough for solution design and governance.
- Production: complete enough for operational support and control evidence.
- Deprecated: clear replacement, retirement, and consumer-impact data.

Governance gates:

- Intake / discovery.
- Solution design approval.
- Build readiness.
- Integration contract approval.
- Release readiness / change approval.
- Deploy to production / go-live.
- Operate / periodic attestation.
- Decommission / retirement.

## Gate-Aware Validation Model

Use validation profiles by gate. The same rule catalog can apply across the lifecycle, but each gate decides which rules are blocking, advisory, or not applicable yet.

| Gate | Purpose | Blocking Focus | Advisory Focus |
|------|---------|----------------|----------------|
| Intake / Discovery | Decide whether the proposed work is understood enough to explore. | Basic identity, requester/sponsor, business outcome, candidate product/system scope. | Initial capability mapping, known existing systems, rough data/integration needs, initial risks. |
| Solution Design Approval | Approve the target design, reuse choices, constraints, and risk posture before build. | Business capability alignment, owning team, target product/system/experience boundaries, required design attributes, key integration surfaces, data classification, applicable standards/regulations/policies/NFRs/controls, major risks, standards fit, known exceptions. | Full machine-readable contracts, final runtime topology, final SLO dashboards, full deployment evidence. |
| Build Readiness | Confirm teams have enough design detail to start implementation safely. | Approved design decision, deployable-unit ownership, repository/scaffold plan, interface-contract intent, data contract draft, NFR targets, required controls mapped to implementation tasks. | Final evidence, final release/change record, production telemetry. |
| Integration Contract Approval | Approve contracts that other teams will consume or depend on. | Interface owner, visibility, producer/consumer intent, schema/payload, protocol, auth pattern, versioning, compatibility policy, data classification. | Final production endpoints, runtime telemetry, incident history. |
| Release Readiness / Change Approval | Confirm release scope, testing, change impact, and operational readiness before production deployment. | Requirement/work item traceability, release contents, test evidence, change record, affected systems/interfaces/data, deployment plan, rollback plan, support readiness. | Post-deploy incident data and long-term SLO trend evidence. |
| Deploy to Production / Go-Live | Confirm production deployment is safe, supportable, observable, and compliant. | Production owner/support, deployment evidence, runtime environment, observability, SLOs, recovery attributes, control evidence or approved exceptions, security/data controls, release/change approval. | Optimization recommendations and non-critical enrichment. |
| Operate / Periodic Attestation | Keep production knowledge current and trustworthy. | Owner/support current, controls/evidence current, exceptions unexpired, critical dependencies known, unsupported technology risks tracked, SLO/incident review coverage. | Portfolio rationalization, modernization opportunities, documentation enrichment. |
| Decommission / Retirement | Safely retire systems, units, interfaces, or data flows. | Replacement/retirement decision, consumer impact, data retention/disposition, control closure, integration shutdown, owner approval, communication plan. | Historical lineage enrichment and lessons learned. |

## Gate Severity Overrides

Some rules change severity by gate:

| Rule Theme | Solution Design Approval | Deploy to Production |
|------------|--------------------------|----------------------|
| Product-to-runtime traceability | Proposed Product -> DigitalExperience -> SoftwareSystem -> DeployableUnit path is required where known; gaps must be explicit. | Actual DeployableUnit -> RuntimeService -> Environment path is required for production scope. |
| Interface contracts | InterfaceContract intent, owner, visibility, producer/consumer, data classification, and contract type are required. | Machine-readable contract, version, endpoint/channel/file location, auth, compatibility policy, and consumer impact are required for public/restricted contracts. |
| Data responsibility | Data domains/entities, classification, owner/steward, system-of-record assumption, residency/retention concerns are required. | Implemented controls, access model, lineage/evidence, retention/residency handling, and monitoring are required. |
| Controls, NFRs, and regulatory applicability | Applicable standards, regulations, policies, constraints, NFRs, controls, risks, and planned evidence are required. | Evidence or approved exception is required for applicable controls, constraints, and NFRs. Open critical risks block unless accepted by authority. |
| SDLC traceability | Requirements and intended work/deployable-unit mapping should be known enough to estimate/build. | Requirement/work item/release/build/deployment traceability is required for production changes. |
| Operations | Target SLOs, support model, recovery requirements, and observability plan are required. | Support team, runbook, alerting/monitoring evidence, SLO instrumentation, recovery evidence, and go-live support are required. |
| Technology standards | Proposed technology choices and standard fit are required. | Actual technology inventory and policy violations/risks are required. |

## Gate Profiles

### Intake / Discovery Gate

Blocking validations:

- Proposed effort has name, sponsor/requester, business problem, expected outcome, and initial owner.
- Candidate product, capability, digital experience, or software-system scope is identified.
- Known existing systems or candidate reuse options are captured, even if incomplete.

Advisory validations:

- Initial integration, data, risk, control, and NFR concerns are listed.
- Known unknowns are captured as discovery questions.

### Solution Design Approval Gate

Blocking validations:

- Product, DigitalExperience, SoftwareSystem, and major DeployableUnit boundaries are defined at the level needed for design approval.
- BusinessCapability, BusinessProcess, or Outcome alignment is present.
- Owning team/accountable owner is identified for the proposed solution and impacted systems.
- Candidate reuse decisions are documented: reuse, extend, replace, build new, or defer.
- Required solution-design attributes are present: lifecycle target, strategic posture, criticality, hosting model, region, availability/latency/volume assumptions, security posture, compliance posture, and support model.
- Key InterfaceContracts are identified with owner, type, producer/consumer, visibility, data exchanged, authentication approach, and versioning approach.
- Sensitive/restricted data is classified, with data owner/steward and residency/retention concerns identified.
- Applicable standards, regulations, policies, constraints, NFRs, controls, risks, and exceptions are identified.
- Applicability rationale is captured for major standards, controls, and NFRs: why each applies, does not apply, or is still under review.
- Planned evidence is identified for standards, controls, and NFRs that cannot yet have implementation evidence.
- Major upstream/downstream impacts are known or explicitly marked as open design risks.

Advisory validations:

- Full OpenAPI/AsyncAPI/schema definitions.
- Final repository/build/deployment traceability.
- Final runtime service inventory.
- Final observability dashboards and production evidence.

### Build Readiness Gate

Blocking validations:

- Approved solution design or documented exception exists.
- DeployableUnit list is stable enough to assign owners and repositories.
- Each custom-built DeployableUnit has a repository, scaffold, or source-location plan.
- InterfaceContract drafts exist for cross-team dependencies.
- DataContract drafts exist for sensitive or cross-system data exchange.
- Required controls, standards, constraints, and NFRs are represented as implementation tasks, acceptance criteria, or test/evidence plans.
- Test strategy covers critical flows, integration points, and control evidence needs.

Advisory validations:

- Release/change records.
- Production telemetry.
- Final control evidence.

### Integration Contract Approval Gate

Blocking validations:

- InterfaceContract has owner, visibility, producer, intended consumers, and lifecycle state.
- Contract type is known: REST, GraphQL, event, stream, queue, file, batch, UI fragment, agent capability, tool contract, or SaaS connector.
- Data exchanged is mapped to DataEntity or DataContract.
- Authentication/authorization pattern is defined.
- Versioning and compatibility policy are defined.
- Sensitive data classification and consumer restrictions are defined.

Advisory validations:

- Complete endpoint/channel location if not available yet.
- Performance baselines if not available yet.

### Release Readiness / Change Approval Gate

Blocking validations:

- Release scope is linked to requirements, work items, changes, artifacts, and affected deployable units.
- Affected InterfaceContracts, DataContracts, systems, consumers, and controls are known.
- Test evidence exists for critical flows and integrations.
- Deployment and rollback plan exist.
- Support, communications, and change-approval responsibilities are assigned.
- Open risks and exceptions are reviewed and accepted or remediated.

Advisory validations:

- Post-production SLO trend evidence.
- Long-term modernization and rationalization opportunities.

### Deploy to Production / Go-Live Gate

Blocking validations:

- Production Deployment links to Release or Artifact, target Environment, timestamp, and responsible actor/system.
- Production DeployableUnit is deployed as RuntimeService.
- RuntimeService runs in Environment and has support team.
- Critical RuntimeService has SLO, observability coverage, alerting, and runbook or approved exception.
- Critical SoftwareSystem has recovery attributes such as RTO/RPO or recovery tier.
- Applicable controls have evidence or approved exception.
- Applicable standards, constraints, and NFRs have evidence, test results, telemetry/SLO proof, or approved exceptions.
- Evidence has provenance.
- Sensitive data controls are implemented or have approved exception.
- Change/release approval exists.

Advisory validations:

- Non-critical enrichment, documentation improvements, optimization recommendations.

### Operate / Periodic Attestation Gate

Blocking validations:

- Owners/support teams are still valid.
- Critical controls have current evidence.
- Exceptions are unexpired or reapproved.
- Unsupported or restricted technology is linked to risk/remediation.
- Critical dependencies and interface consumers are current enough for incident/change impact.

Advisory validations:

- Portfolio rationalization.
- Modernization candidate scoring.
- Documentation quality.

### Decommission / Retirement Gate

Blocking validations:

- Replacement, retirement, or no-longer-needed decision is approved.
- Consumers and downstream dependencies are identified and notified.
- InterfaceContracts have retirement dates and replacement guidance.
- Data retention/disposition requirements are satisfied.
- Controls, risks, evidence, and exceptions are closed or transferred.
- Runtime services, integrations, monitoring, and support routes are retired.

Advisory validations:

- Historical lineage and lessons learned captured.

## Gate Profile Representation

Represent each gate as a validation profile, not as a separate ontology.

```text
ValidationProfile
  name: Solution Design Approval
  appliesTo: Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, DataEntity, Risk, Control
  rules:
    - rule: SoftwareSystem has accountable owner
      severity: error
    - rule: InterfaceContract has machine-readable definition
      severity: warning
    - rule: Applicable controls have evidence
      severity: warning
```

```text
ValidationProfile
  name: Deploy to Production
  appliesTo: DeployableUnit, RuntimeService, Deployment, Environment, Release, Control, Evidence
  rules:
    - rule: Production DeployableUnit is deployed as RuntimeService
      severity: error
    - rule: InterfaceContract has machine-readable definition
      severity: error
    - rule: Applicable controls have evidence
      severity: critical
```

This keeps the rule catalog stable while allowing each gate to apply different severity and evidence requirements.

## Validation Rule Families

| Family | Purpose |
|--------|---------|
| Identity and naming | Ensure every entity can be uniquely referenced and understood. |
| Ownership and accountability | Ensure managed objects have accountable owners. |
| Product-to-runtime completeness | Ensure the accepted stack is traceable. |
| Normative applicability | Ensure applicable standards, regulations, policies, constraints, controls, and NFRs are identified at the right gate. |
| Solution-design attributes | Ensure systems and interfaces expose design-relevant facts. |
| Integration contracts | Ensure APIs, events, files, queues, streams, UI fragments, and agent capabilities have contracts. |
| Data responsibility | Ensure mastered/consumed data is classified and governed. |
| SDLC traceability | Ensure requirements can be traced to work, code, builds, releases, and deployments. |
| Runtime and operations | Ensure production systems have support, SLOs, observability, and recovery attributes. |
| Risk and compliance | Ensure controls, risks, evidence, exceptions, and standards are connected. |
| Source authority and provenance | Ensure facts and relationships have traceable sources and authority status. |

## Core Validation Rules

The severities below are default rule severities. Gate profiles may override them. For example, a missing machine-readable API definition may be a warning at solution design approval but an error at integration contract approval or deploy to production.

### Identity and Naming

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Every canonical entity has a stable identifier. | All core entities | Error | Missing canonical ID. |
| Every canonical entity has a human-readable name. | All core entities | Error | Missing name. |
| Every canonical entity has a short description. | Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, DataEntity, Control | Warning | Missing description. |
| Source identifiers are preserved. | Imported entities | Warning | Missing source system or source record ID. |

### Ownership and Accountability

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Product has an accountable owner. | Product | Error | No owns/accountableFor relationship to Team, Person, or Organization. |
| SoftwareSystem has an accountable owner. | SoftwareSystem | Error | No owns relationship to Team, Person, or Organization. |
| DeployableUnit has a technical owner. | DeployableUnit | Error for production, warning otherwise | No owns relationship to Team or Person. |
| InterfaceContract has an owning team. | InterfaceContract | Error for public/restricted contracts, warning for private contracts | No owning team. |
| DataEntity has a data owner or steward when classified sensitive/restricted. | DataEntity | Critical | Sensitive/restricted data has no owner or steward. |
| Control has a control owner. | Control | Error | No control owner. |

### Product-to-Runtime Completeness

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Product is connected to at least one value anchor. | Product | Warning | No supportsCapability, realizesOutcome, or enablesProcess path. |
| Product has at least one realization path. | Product | Warning | No realizedBy SoftwareSystem and no experiencedThrough DigitalExperience. |
| DigitalExperience is composed from deployable units or external systems. | DigitalExperience | Warning | No composedFrom relationship. |
| SoftwareSystem is composed of deployable units or has an approved exception. | SoftwareSystem | Warning | No composedOf DeployableUnit and no exception. |
| Production DeployableUnit is deployed as a RuntimeService. | DeployableUnit | Error | Production unit has no deployedAs RuntimeService. |
| RuntimeService runs in at least one Environment. | RuntimeService | Error | No runsIn Environment. |

### Solution-Design Attributes

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| SoftwareSystem has lifecycle state. | SoftwareSystem | Error | Missing lifecycle state. |
| SoftwareSystem has strategic posture. | SoftwareSystem | Warning | Missing strategic/tactical/deprecated/restricted posture. |
| Production SoftwareSystem has criticality. | SoftwareSystem | Error | Missing business or operational criticality. |
| DeployableUnit has deployment/runtime pattern. | DeployableUnit | Warning | Missing kind or runtime pattern. |
| Production system has hosting model and region. | SoftwareSystem, RuntimeService | Error | Missing hosting model or region. |
| Reusable InterfaceContract has visibility. | InterfaceContract | Error | Missing public/restricted/private visibility. |

### Integration Contracts

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| InterfaceContract has at least one concrete surface. | InterfaceContract | Error | No realizesSurface IntegrationSurface. |
| API contract has machine-readable definition. | API, InterfaceContract | Error for public/restricted, warning for private | Missing OpenAPI/GraphQL/protobuf/etc. definition reference. |
| Event contract has machine-readable definition. | EventStream, InterfaceContract | Error for public/restricted, warning for private | Missing AsyncAPI/schema/channel definition. |
| File/batch contract has schema and transfer metadata. | FileTransfer, BatchFeed | Warning | Missing schema, cadence, transport, or owner. |
| InterfaceContract has producer and consumer relationships when known. | InterfaceContract | Warning | Missing exposing or consuming system/unit. |
| Deprecated InterfaceContract has replacement or retirement date. | InterfaceContract | Error | Deprecated without replacement, retirement date, or exception. |

### Data Responsibility

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| DataEntity exchanged across interfaces has classification. | DataEntity | Error | Missing data classification. |
| Sensitive data has residency and retention attributes. | DataEntity, DataAsset | Critical | Sensitive/restricted data missing residency or retention policy. |
| Mastered data has a system of record. | DataEntity | Error | No mastersData SoftwareSystem relationship. |
| Derived DataAsset has lineage. | DataAsset | Warning | Missing derivesData relationship. |
| AI/RAG-used data has usage constraints. | DataAsset, DataEntity | Warning | Missing approved usage, access, or sensitivity constraints. |

### SDLC Traceability

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Requirement links to implementation work. | Requirement | Warning | No implementedBy WorkItem. |
| WorkItem that changes production has target. | WorkItem | Error | No changes target. |
| DeployableUnit links to repository or source exception. | DeployableUnit | Warning | No implementedIn Repository and no vendor/COTS exception. |
| Release has included changes or artifacts. | Release | Error | No includedInRelease sources. |
| Deployment has target environment and timestamp. | Deployment | Error | Missing deployedTo Environment or timestamp. |
| Production deployment links to release or artifact. | Deployment | Error | No deployedBy Release/Artifact relationship. |

### Runtime and Operations

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Production RuntimeService has support team. | RuntimeService | Error | No supportedBy Team. |
| Critical RuntimeService has SLO or approved exception. | RuntimeService | Error | Critical service has no governedBySLO and no exception. |
| Production RuntimeService has observability coverage. | RuntimeService | Warning | Missing metric/trace/log/alert relationship. |
| Critical SoftwareSystem has recovery attributes. | SoftwareSystem | Error | Missing RTO/RPO or recovery tier. |
| Incident is linked to affected target. | Incident | Warning | No affects relationship. |
| Major incident is linked to cause, change, deployment, or post-incident review when known. | Incident | Warning | Missing causedBy or review evidence. |

### Risk and Compliance

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Critical system has applicable controls or control exception. | SoftwareSystem | Critical | No Control appliesTo relationship and no exception. |
| Control has evidence or accepted exception. | Control | Error | No satisfiedBy Evidence and no exception. |
| Evidence has provenance. | Evidence | Error | Missing source, timestamp, or assertedBy. |
| Unsupported technology creates or links to risk. | TechnologyComponent, SoftwareSystem, DeployableUnit | Warning | Deprecated/restricted technology without Risk. |
| Open risk has owner and treatment. | Risk | Error | Missing owner, status, or treatment plan. |
| Exception has approver and expiration. | Exception | Error | Missing approvedBy or validTo. |

### Normative Applicability

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Applicable standards are identified for solution-design targets. | Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, DataEntity, RuntimeService | Error at solution design approval | No applicable Standard/Policy/Regulation/Constraint set and no rationale. |
| Applicability rule has target condition. | ApplicabilityRule | Error | Missing target class, attribute condition, relationship condition, lifecycle state, or gate scope. |
| Constraint has normative source or decision source. | Constraint | Warning | Constraint is not derivedFrom NormativeSource/Standard/Policy/Regulation/ArchitectureDecision. |
| QualityAttributeRequirement has measurable target. | QualityAttributeRequirement | Error for build readiness and production | Missing metric, threshold, unit, or acceptance method. |
| Regulation-derived control has owner. | Control | Critical | Control derivedFrom Regulation/Policy but has no control owner. |
| Required NFR has planned evidence before build. | QualityAttributeRequirement | Warning at solution design, error at build readiness | No planned Evidence/TestCase/SLO/Metric. |
| Required NFR has actual evidence before production. | QualityAttributeRequirement | Error at deploy to production | No requirementSatisfiedBy Evidence/TestCase/SLO/Metric or exception. |
| Exception is time-boxed and approved. | Exception | Error | Missing approvedBy, validTo, or risk impact. |

### Source Authority and Provenance

| Rule | Target | Severity | Condition |
|------|--------|----------|-----------|
| Imported relationship has source metadata. | Relationship assertion | Warning | Missing sourceSystem, sourceRecordId, assertedAt. |
| Authoritative field has declared record authority. | Governed attributes | Error | Missing source-of-truth declaration. |
| Conflicting authoritative facts are flagged. | Governed attributes | Error | Multiple authoritative values without conflict resolution. |
| Inferred relationships remain reviewable. | Inferred relationship assertions | Info | Missing confidence or reviewStatus. |

## Example SHACL-Style Shapes

These are illustrative, not final syntax.

```turtle
ukb:SoftwareSystemShape
  a sh:NodeShape ;
  sh:targetClass ukb:SoftwareSystem ;
  sh:property [
    sh:path ukb:canonicalId ;
    sh:minCount 1 ;
    sh:severity sh:Violation ;
  ] ;
  sh:property [
    sh:path ukb:ownedBy ;
    sh:minCount 1 ;
    sh:class ukb:Team ;
    sh:severity sh:Violation ;
  ] ;
  sh:property [
    sh:path ukb:lifecycleState ;
    sh:minCount 1 ;
    sh:in ( ukb:Draft ukb:Candidate ukb:Approved ukb:Production ukb:Deprecated ukb:Retired ) ;
    sh:severity sh:Violation ;
  ] .
```

```turtle
ukb:InterfaceContractShape
  a sh:NodeShape ;
  sh:targetClass ukb:InterfaceContract ;
  sh:property [
    sh:path ukb:realizesSurface ;
    sh:minCount 1 ;
    sh:severity sh:Violation ;
  ] ;
  sh:property [
    sh:path ukb:visibility ;
    sh:minCount 1 ;
    sh:in ( ukb:Public ukb:Restricted ukb:Private ) ;
    sh:severity sh:Violation ;
  ] .
```

## Validation Operating Model

Validation should run in several places:

- During ingestion from source systems.
- During ontology pull requests or change proposals.
- During solution-design review.
- During architecture governance review.
- During release/deployment evidence collection.
- During periodic data-quality scans.
- Before AI/RAG retrieval indexes are refreshed.

Validation outputs should be usable by humans:

- Error summary.
- Affected product/system/unit/interface.
- Missing or invalid fact.
- Source system responsible.
- Suggested owner.
- Required remediation or exception path.
- Link to source evidence.

## Phase 7 Deliverables

The reviewed output of this phase should be:

- Validation rule families.
- Initial rule catalog with severity.
- Gate-specific validation profiles.
- Lifecycle-aware validation expectations.
- SHACL-style shape examples.
- Exception and remediation pattern.
- Source-authority and provenance validation rules.
- A priority subset for the first pilot.

## Review Questions

- Which rules should be blocking versus advisory?
- Which rules are mandatory for solution design?
- Which rules are mandatory for build readiness, release readiness, and integration contract approval?
- Which rules are mandatory only for production systems?
- Which gates should allow explicit open risks versus require approved exceptions?
- Which rules should be enforced through SHACL, and which should remain governance checks?
- Which attributes need controlled vocabularies before validation can work?
- How should exceptions be approved, time-boxed, and represented?

## Research Anchors for Progressive Disclosure

Use these anchors when explaining validation design choices.

| Anchor | Use When |
|--------|----------|
| [SHACL](https://www.w3.org/TR/shacl/) | Explaining graph validation rules, required properties, allowed values, and severity. |
| [PROV-O](https://www.w3.org/TR/prov-overview/) | Explaining provenance validation for evidence, assertions, and source-system lineage. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining validation expectations for HTTP API contract completeness. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining validation expectations for event/message/channel contracts. |
| [COBIT](https://www.isaca.org/resources/cobit) | Explaining control, assurance, risk, accountability, and governance validation needs. |
| [OpenTelemetry](https://opentelemetry.io/docs/concepts/) | Explaining observability coverage and runtime evidence. |

Progressive-disclosure rule:

- Start with the rule family and severity.
- Show SHACL-like examples only when the team wants to see how validation becomes executable.
- Bring in provenance and control anchors when discussing trust, auditability, or exceptions.
