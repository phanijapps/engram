---
type: ontology-phase
title: Phase 11 - Worked Pilot Instance
slug: it-sdlc-ontology-phase-11-worked-pilot-instance
project: unified-knowledge-base
phase: 11
status: drafted
provenance: ai-assisted
created: 2026-06-29
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - it-sdlc-ontology
  - worked-example
  - pilot-instance
  - solution-design-brief
---

## Purpose

Phase 11 instantiates the minimum viable ontology specification from Phase 10 with a concrete worked pilot example.

The goal is to show how ontology records become a human-consumable **Solution Design Brief View** for the **Solution Design Approval** gate.

This phase should answer:

> If we had a small reviewed slice of ontology facts, what would the pilot records look like and how would they assemble into a solution design packet?

## Example Caveat

This is a fictionalized example for ontology design.

It is not a claim about any real enterprise system, source record, architecture decision, control status, or production implementation.

Use it to test whether the ontology shape is understandable, complete enough, and reviewable.

## Pilot Slice

```text
Pilot name: Participant Retirement Advice - Solution Design Brief
Target gate: Solution Design Approval
Primary anchor: DigitalExperience
Primary consumer output: Solution Design Brief View
Primary scenario: Add guided retirement advice planning with web, BFF, backend, event, and agentic explanation components.
```

## Pilot Competency Questions

The worked instance should answer these first:

1. What product, digital experience, and software system are in scope?
2. Which deployable units compose the experience?
3. Which interface contracts are exposed or consumed?
4. Which data entities cross system or trust boundaries?
5. Which teams own the product, system, deployable units, contracts, data, controls, and reviews?
6. Which required design attributes are complete, missing, or advisory?
7. Which standards, controls, NFRs, risks, and exceptions apply?
8. Which facts are reviewed, authoritative, candidate, inferred, disputed, or planned?
9. Which findings block Solution Design Approval?
10. Which planned evidence should move to Build Readiness or Deploy to Production gates?

## Source Profiles For Example

| Source Profile | Authority Scope | Mapped Concepts | Pilot Use |
|----------------|-----------------|-----------------|-----------|
| `sp-architecture-repository` | authoritative for reviewed architecture records | Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, Decision, Evidence | Seeded solution architecture facts and review notes. |
| `sp-sdlc-tracker` | supporting for work and initiative scope | Requirement, WorkItem, Team, Evidence | Links design scope to delivery planning. |
| `sp-api-catalog` | candidate for interface contracts until reviewed | InterfaceContract, DataEntity, Evidence | Supplies API/event contract candidates. |
| `sp-repository-catalog` | supporting for repository mappings | Repository, DeployableUnit, Evidence | Links deployable units to implementation repositories. |
| `sp-grc-catalog` | authoritative for controls and control applicability where reviewed | NormativeSource, Control, Constraint, Evidence, Exception | Supplies policy/control applicability and review status. |

## Teams And Owners

| ontologyId | Class | name | Role In Pilot | factState |
|------------|-------|------|---------------|-----------|
| `team-product-advice` | Team | Advice Product Team | Product owner and experience sponsor. | reviewed_fact |
| `team-architecture-advice` | Team | Advice Architecture Team | Solution architecture owner. | reviewed_fact |
| `team-engineering-advice-web` | Team | Advice Web Engineering | Owns micro-frontend and BFF. | reviewed_fact |
| `team-engineering-advice-platform` | Team | Advice Platform Engineering | Owns backend service and event publisher. | reviewed_fact |
| `team-data-retirement` | Team | Retirement Data Stewardship | Owns data classification and mastership review. | candidate_fact |
| `team-risk-control` | Team | Risk and Control Review | Owns control applicability review. | candidate_fact |
| `team-operations-advice` | Team | Advice Operations | Owns production support model review. | candidate_fact |

## Core Landscape Records

### Business Capability

| Property | Value |
|----------|-------|
| ontologyId | `bc-retirement-guidance` |
| name | Retirement Guidance |
| description | Ability to help participants understand retirement readiness and available planning actions. |
| ownerTeam | `team-product-advice` |
| factState | reviewed_fact |
| sourceProfileId | `sp-architecture-repository` |

### Product

| Property | Value |
|----------|-------|
| ontologyId | `prod-participant-retirement-advice` |
| name | Participant Retirement Advice |
| ownerTeam | `team-product-advice` |
| supportsBusinessCapability | `bc-retirement-guidance` |
| hasDigitalExperience | `dx-participant-web-advice` |
| businessCriticality | high |
| lifecycleState | active |
| factState | reviewed_fact |
| sourceProfileId | `sp-architecture-repository` |

### DigitalExperience

| Property | Value |
|----------|-------|
| ontologyId | `dx-participant-web-advice` |
| name | Participant Web Advice Experience |
| experienceKind | web-self-service |
| channel | web |
| realizesProduct | `prod-participant-retirement-advice` |
| composedOfSoftwareSystem | `sys-advice-orchestration` |
| ownerTeam | `team-product-advice` |
| consumerActor | participant |
| factState | reviewed_fact |
| sourceProfileId | `sp-architecture-repository` |

### Software System

| Property | Value |
|----------|-------|
| ontologyId | `sys-advice-orchestration` |
| name | Advice Orchestration System |
| systemPurpose | Coordinates retirement advice planning interactions, recommendation generation, and explanation delivery. |
| ownerTeam | `team-architecture-advice` |
| lifecycleState | active |
| realizesProduct | `prod-participant-retirement-advice` |
| supportsBusinessCapability | `bc-retirement-guidance` |
| containsDeployableUnit | `du-advice-goal-planning-mfe`, `du-advice-web-bff`, `du-advice-orchestration-service`, `du-recommendation-agent-service`, `du-advice-event-publisher` |
| exposesInterfaceContract | `ic-advice-plan-api`, `ic-advice-recommendation-event` |
| consumesInterfaceContract | `ic-participant-profile-api`, `ic-agent-explanation-tool` |
| hasRequiredAttributeSet | `solution-design-approval-minimum` |
| factState | reviewed_fact |
| sourceProfileId | `sp-architecture-repository` |

## Deployable Units

| ontologyId | name | deployableUnitKind | ownedByTeam | partOfSoftwareSystem | repository | factState |
|------------|------|--------------------|-------------|----------------------|------------|-----------|
| `du-advice-goal-planning-mfe` | Advice Goal Planning Micro-Frontend | micro-frontend | `team-engineering-advice-web` | `sys-advice-orchestration` | `repo-advice-goal-planning-mfe` | reviewed_fact |
| `du-advice-web-bff` | Advice Web BFF | bff | `team-engineering-advice-web` | `sys-advice-orchestration` | `repo-advice-web-bff` | reviewed_fact |
| `du-advice-orchestration-service` | Advice Orchestration Service | backend-service | `team-engineering-advice-platform` | `sys-advice-orchestration` | `repo-advice-orchestration-service` | reviewed_fact |
| `du-recommendation-agent-service` | Recommendation Agent Service | agent-service | `team-engineering-advice-platform` | `sys-advice-orchestration` | `repo-recommendation-agent-service` | candidate_fact |
| `du-advice-event-publisher` | Advice Event Publisher | event-publisher | `team-engineering-advice-platform` | `sys-advice-orchestration` | `repo-advice-event-publisher` | reviewed_fact |

## Interface Contracts

| ontologyId | name | interfaceKind | provider | consumer | dataEntityExchanged | contractStatus | factState |
|------------|------|---------------|----------|----------|---------------------|----------------|-----------|
| `ic-advice-plan-api` | Advice Plan API | rest-api | `du-advice-orchestration-service` | `du-advice-web-bff` | `de-retirement-goal`, `de-advice-recommendation` | proposed | reviewed_fact |
| `ic-participant-profile-api` | Participant Profile API | rest-api | external-profile-system | `du-advice-orchestration-service` | `de-participant`, `de-account-balance` | existing | candidate_fact |
| `ic-advice-recommendation-event` | Advice Recommendation Event | event | `du-advice-event-publisher` | downstream-analytics-consumers | `de-advice-recommendation` | proposed | candidate_fact |
| `ic-agent-explanation-tool` | Advice Explanation Tool Contract | agent-tool-contract | `du-recommendation-agent-service` | `du-advice-orchestration-service` | `de-advice-recommendation`, `de-retirement-goal` | proposed | candidate_fact |

## Data Entities And Assets

| ontologyId | name | dataOwner | dataClassification | masteredBy / systemOfRecord | Used Or Exchanged By | factState |
|------------|------|-----------|--------------------|-----------------------------|----------------------|-----------|
| `de-participant` | Participant | `team-data-retirement` | restricted-personal | external-profile-system | `ic-participant-profile-api` | candidate_fact |
| `de-account-balance` | Account Balance | `team-data-retirement` | confidential-financial | external-recordkeeping-system | `ic-participant-profile-api` | candidate_fact |
| `de-retirement-goal` | Retirement Goal | `team-product-advice` | confidential | `sys-advice-orchestration` | `ic-advice-plan-api`, `ic-agent-explanation-tool` | reviewed_fact |
| `de-advice-recommendation` | Advice Recommendation | `team-product-advice` | confidential-advice | `sys-advice-orchestration` | `ic-advice-plan-api`, `ic-advice-recommendation-event`, `ic-agent-explanation-tool` | reviewed_fact |

## Standards, NFRs, Controls, Risks, And Exceptions

### Normative Sources

| ontologyId | name | Source Type | factState |
|------------|------|-------------|-----------|
| `ns-enterprise-api-standard` | Enterprise API Standard | architecture-standard | reviewed_fact |
| `ns-enterprise-data-classification-policy` | Enterprise Data Classification Policy | policy | candidate_fact |
| `ns-model-agent-review-standard` | Model and Agent Review Standard | control-standard | candidate_fact |
| `ns-production-readiness-standard` | Production Readiness Standard | operational-standard | candidate_fact |

### Requirements, Controls, And NFRs

| ontologyId | Class | name | appliesTo | gateRelevance | requiredEvidence | factState |
|------------|-------|------|-----------|---------------|------------------|-----------|
| `qar-advice-api-latency` | QualityAttributeRequirement | Advice API latency target | `ic-advice-plan-api` | Solution Design Approval | latency assumption and test plan | candidate_fact |
| `qar-advice-availability` | QualityAttributeRequirement | Advice experience availability target | `dx-participant-web-advice` | Solution Design Approval | NFR review note | candidate_fact |
| `ctrl-data-classification-review` | Control | Data classification review | `de-participant`, `de-account-balance`, `de-advice-recommendation` | Solution Design Approval | data steward review | candidate_fact |
| `ctrl-agent-output-review` | Control | Agent output review and explanation control | `du-recommendation-agent-service`, `ic-agent-explanation-tool` | Solution Design Approval | agent review decision and risk acceptance | candidate_fact |
| `constraint-api-standard` | Constraint | API contracts must use approved versioning and authentication patterns | `ic-advice-plan-api`, `ic-participant-profile-api` | Solution Design Approval | API catalog review | reviewed_fact |

### Risks And Exceptions

| ontologyId | Class | name | target | status | severity | ownerTeam | factState |
|------------|-------|------|--------|--------|----------|-----------|-----------|
| `risk-agent-explanation-trust` | Risk | Agent explanation may produce unclear or overly personalized rationale | `du-recommendation-agent-service` | open | high | `team-risk-control` | candidate_fact |
| `risk-profile-api-dependency` | Risk | Participant Profile API dependency ownership not yet reviewed | `ic-participant-profile-api` | open | medium | `team-architecture-advice` | candidate_fact |
| `exception-none-recorded` | Exception | No approved exceptions recorded for pilot slice | pilot-scope | not_applicable | info | `team-architecture-advice` | reviewed_fact |

## Evidence Records

| ontologyId | evidenceKind | name | supportsClaim | sourceProfileId | reviewStatus | factState |
|------------|--------------|------|---------------|-----------------|--------------|-----------|
| `ev-solution-architecture-note-001` | architecture-review | Initial advice solution architecture note | system composition and product scope | `sp-architecture-repository` | reviewed | reviewed_fact |
| `ev-api-catalog-advice-plan-draft` | api-spec | Draft Advice Plan API catalog entry | `ic-advice-plan-api` | `sp-api-catalog` | reviewed | reviewed_fact |
| `ev-event-contract-advice-rec-draft` | event-spec | Draft Advice Recommendation Event contract | `ic-advice-recommendation-event` | `sp-api-catalog` | pending-review | candidate_fact |
| `ev-data-classification-request-001` | data-classification | Data classification review request | data classifications for participant/account/advice data | `sp-grc-catalog` | pending-review | candidate_fact |
| `ev-agent-review-request-001` | manual-review-note | Agent review intake note | agent explanation control applicability | `sp-grc-catalog` | pending-review | candidate_fact |

## Relationship Instances

| Relationship | Source | Target | factState |
|--------------|--------|--------|-----------|
| supportsCapability | `prod-participant-retirement-advice` | `bc-retirement-guidance` | reviewed_fact |
| realizesProduct | `dx-participant-web-advice` | `prod-participant-retirement-advice` | reviewed_fact |
| realizesProduct | `sys-advice-orchestration` | `prod-participant-retirement-advice` | reviewed_fact |
| containsDeployableUnit | `sys-advice-orchestration` | `du-advice-goal-planning-mfe` | reviewed_fact |
| containsDeployableUnit | `sys-advice-orchestration` | `du-advice-web-bff` | reviewed_fact |
| containsDeployableUnit | `sys-advice-orchestration` | `du-advice-orchestration-service` | reviewed_fact |
| containsDeployableUnit | `sys-advice-orchestration` | `du-recommendation-agent-service` | candidate_fact |
| exposesInterfaceContract | `du-advice-orchestration-service` | `ic-advice-plan-api` | reviewed_fact |
| consumesInterfaceContract | `du-advice-orchestration-service` | `ic-participant-profile-api` | candidate_fact |
| exposesInterfaceContract | `du-advice-event-publisher` | `ic-advice-recommendation-event` | candidate_fact |
| consumesInterfaceContract | `du-advice-orchestration-service` | `ic-agent-explanation-tool` | candidate_fact |
| exchangesDataEntity | `ic-participant-profile-api` | `de-participant` | candidate_fact |
| exchangesDataEntity | `ic-participant-profile-api` | `de-account-balance` | candidate_fact |
| exchangesDataEntity | `ic-advice-plan-api` | `de-retirement-goal` | reviewed_fact |
| exchangesDataEntity | `ic-advice-plan-api` | `de-advice-recommendation` | reviewed_fact |
| appliesTo | `ctrl-data-classification-review` | `de-participant` | candidate_fact |
| appliesTo | `ctrl-agent-output-review` | `du-recommendation-agent-service` | candidate_fact |
| supportedByEvidence | `ic-advice-plan-api` | `ev-api-catalog-advice-plan-draft` | reviewed_fact |

## Generated Solution Design Brief View

### Brief Header

| Field | Value |
|-------|-------|
| Brief name | Participant Retirement Advice - Solution Design Brief |
| Target gate | Solution Design Approval |
| Anchor | `dx-participant-web-advice` |
| Product | Participant Retirement Advice |
| Primary software system | Advice Orchestration System |
| Current approval posture | Not ready for approval until blocking findings are resolved. |
| Fact posture | Mixed: reviewed scope/composition, candidate data/control/agent facts. |

### Context Selection Trace

| Trace Field | Value |
|-------------|-------|
| Anchor | `dx-participant-web-advice` |
| Target gate | Solution Design Approval |
| Retrieval modes used | Stable semantic context, scoped graph retrieval, rule-triggered controls/NFRs, evidence drill-down, and elicitation prompts. |
| Fact states allowed | `reviewed_fact` in the main design posture; `candidate_fact` only when visibly marked as caveat, finding, or decision needed. |
| Candidate facts included | External dependency ownership, downstream event consumers, participant/account-balance data classification, agent control applicability, latency target, and availability target. |
| Facts excluded | Full production runtime telemetry, full deploy-to-production evidence, and unreviewed agentic-system extension details. |
| Freshness warnings | Automated freshness is not proven in this fictional pilot; source-profile freshness must be added before live pilot execution. |
| Elicitation requests | API provider obligation, approved data classification, agent-control applicability, NFR targets, and event release scope. |

### Design Summary

The proposed solution adds guided retirement advice planning to a participant-facing web experience.

The experience is modeled as a Product realized through a DigitalExperience and supported by the Advice Orchestration System. The system is composed of a micro-frontend, BFF, backend orchestration service, recommendation agent service, and event publisher.

The core integration surfaces are the Advice Plan API, Participant Profile API, Advice Recommendation Event, and Advice Explanation Tool Contract. The design touches participant, account balance, retirement goal, and advice recommendation data.

### Scope

In scope:

- Participant Web Advice Experience.
- Advice Orchestration System.
- Advice goal planning micro-frontend.
- Advice web BFF.
- Advice orchestration backend service.
- Recommendation agent service.
- Advice event publisher.
- Advice Plan API.
- Participant Profile API dependency.
- Advice Recommendation Event.
- Advice Explanation Tool Contract.

Out of scope for this pilot:

- Full production runtime evidence.
- Complete deploy-to-production readiness.
- Complete agentic systems extension.
- Complete data lineage beyond the named data entities.
- Full control attestation.

### Application Landscape Context

| Element | Ontology Record | Status |
|---------|-----------------|--------|
| Product | `prod-participant-retirement-advice` | reviewed_fact |
| DigitalExperience | `dx-participant-web-advice` | reviewed_fact |
| SoftwareSystem | `sys-advice-orchestration` | reviewed_fact |
| External dependency | external-profile-system via `ic-participant-profile-api` | candidate_fact |
| Downstream consumers | downstream-analytics-consumers via `ic-advice-recommendation-event` | candidate_fact |

Landscape finding:

- The internal product/system composition is reviewed enough for solution design discussion.
- External dependency ownership and consumer review are not yet complete.

### Product/System Composition

```text
Participant Retirement Advice
  -> Participant Web Advice Experience
    -> Advice Orchestration System
      -> Advice Goal Planning Micro-Frontend
      -> Advice Web BFF
      -> Advice Orchestration Service
      -> Recommendation Agent Service
      -> Advice Event Publisher
```

Interpretation:

- The micro-frontend is the user-facing deployable unit.
- The BFF is the channel-specific backend.
- The orchestration service owns advice workflow coordination.
- The recommendation agent service is an agent-service deployable unit and should be extended later by the agentic systems ontology.
- The event publisher emits advice recommendation events for downstream consumption.

### Integration Surface Summary

| Contract | Type | Provider | Consumer | Status | Gate Impact |
|----------|------|----------|----------|--------|-------------|
| Advice Plan API | rest-api | Advice Orchestration Service | Advice Web BFF | reviewed draft | usable for design approval with API review evidence |
| Participant Profile API | rest-api | external-profile-system | Advice Orchestration Service | candidate existing | dependency ownership and data classification review needed |
| Advice Recommendation Event | event | Advice Event Publisher | downstream analytics consumers | candidate proposed | event consumer review needed |
| Advice Explanation Tool Contract | agent-tool-contract | Recommendation Agent Service | Advice Orchestration Service | candidate proposed | agent review needed |

### Data Summary

| Data Entity | Classification | Owner | System Of Record | Gate Impact |
|-------------|----------------|-------|------------------|-------------|
| Participant | restricted-personal | candidate owner | external-profile-system | blocking until reviewed |
| Account Balance | confidential-financial | candidate owner | external-recordkeeping-system | blocking until reviewed |
| Retirement Goal | confidential | reviewed owner | Advice Orchestration System | acceptable for design |
| Advice Recommendation | confidential-advice | reviewed owner | Advice Orchestration System | acceptable if agent review is completed |

### Standards, Controls, And NFR Summary

| Item | Applies To | Status | Gate Impact |
|------|------------|--------|-------------|
| API versioning and authentication constraint | Advice Plan API, Participant Profile API | reviewed_fact for Advice Plan API, candidate for Profile API | advisory until Profile API review is complete |
| Data classification review | Participant, Account Balance, Advice Recommendation | candidate_fact | blocking for participant and account balance |
| Agent output review and explanation control | Recommendation Agent Service, Advice Explanation Tool Contract | candidate_fact | blocking if agent service remains in Solution Design scope |
| API latency target | Advice Plan API | candidate_fact | advisory if latency target is accepted as planned evidence |
| Availability target | Participant Web Advice Experience | candidate_fact | advisory until NFR owner confirms |

### Validation Findings

| Finding | Severity | Rationale | Owner |
|---------|----------|-----------|-------|
| Participant and Account Balance data classifications are candidate facts, not reviewed facts. | blocking | Personal/financial data crosses system boundaries. Design approval needs reviewed classification or explicit risk acceptance. | `team-data-retirement` |
| Agent output review control applicability is not reviewed. | blocking | Agent service is in scope and may influence participant-facing explanations. | `team-risk-control` |
| Participant Profile API provider ownership has not been reviewed. | advisory | Dependency is known, but provider obligation and consumer expectation need confirmation. | `team-architecture-advice` |
| Advice Recommendation Event consumers are candidate only. | advisory | Event publication design can proceed, but consumer list and event contract ownership need review. | `team-engineering-advice-platform` |
| Production monitoring evidence is not present. | advisory | Expected at later gate, not blocking for Solution Design Approval if support assumptions are documented. | `team-operations-advice` |
| No approved exceptions recorded. | info | No deviation is currently approved or requested. | `team-architecture-advice` |

### Approval Posture

Current recommendation:

> Do not approve yet. Resolve the two blocking findings or convert them into explicit accepted risks with named approvers, review dates, and planned evidence.

Approval can move forward when:

- Data classification for Participant and Account Balance is reviewed or explicitly risk-accepted.
- Agent output review/control applicability is resolved for the recommendation agent service.
- Advisory findings have named owners and planned evidence for later gates.

### Decisions Needed At Gate

1. Is the recommendation agent service in scope for the initial release, or should it move to a later increment?
2. Who owns the Participant Profile API provider obligation for this design?
3. What data classification is approved for Participant and Account Balance use in this flow?
4. What NFR targets are required for Solution Design Approval versus Build Readiness?
5. Is the Advice Recommendation Event required for the first release or only for analytics enablement later?

## What This Example Proves

This worked instance shows that the ontology can:

- Keep product, experience, system, deployable unit, interface contract, data, control, and evidence concepts distinct.
- Represent micro-frontend, BFF, backend service, event publisher, and agent service without collapsing them into "application."
- Generate a useful design brief from ontology facts.
- Show reviewed versus candidate facts.
- Surface blocking and advisory findings by gate.
- Preserve source profile and evidence posture.
- Identify which facts should move to Build Readiness and Deploy to Production later.

## What The Example Does Not Prove Yet

The example does not yet prove:

- Physical implementation architecture.
- Real source-system ingestion.
- Automated validation execution.
- Full data lineage.
- Full risk/control evidence lifecycle.
- Production runtime integration.
- AI/RAG answer quality over the ontology.

Those should be addressed in later phases or pilot execution.

## Phase 11 Deliverables

The reviewed output of this phase should be:

- Worked pilot scope.
- Example source profiles.
- Example team/owner records.
- Example product, experience, system, deployable unit, interface contract, data, control, risk, exception, and evidence records.
- Example relationship instances.
- Generated Solution Design Brief View.
- Gate validation findings.
- Approval posture.
- Decisions needed at the gate.
- Review notes on whether the example is understandable and useful.

## Review Questions

- Is Participant Retirement Advice a good fictional pilot slice, or should the example use a more neutral product name?
- Are the example records concrete enough for stakeholders to understand?
- Does the Solution Design Brief View show the right information for Solution Design Approval?
- Are the blocking versus advisory findings correct?
- Does the example make agentic services clear without overloading the core ontology?
- Does the example show the value of reviewed/candidate/disputed fact states?
- Which fields or relationships feel too heavy for a first pilot?
- What should be changed before using this as the reference example?

## Research Anchors for Progressive Disclosure

Use these anchors when explaining the worked pilot instance.

| Anchor | Use When |
|--------|----------|
| [ISO/IEC/IEEE 42010](https://www.iso.org/standard/74393.html) | Explaining why the Solution Design Brief is a view assembled for specific stakeholders and concerns. |
| [C4 Model](https://c4model.com/) | Explaining product/system/deployable composition in human-readable architecture terms. |
| [Backstage System Model](https://backstage.io/docs/features/software-catalog/system-model/) | Explaining catalog-style records for systems, components, APIs, resources, owners, and domains. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining API contract evidence. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining event contract evidence. |
| [SHACL](https://www.w3.org/TR/shacl/) | Explaining blocking/advisory validation findings. |
| [PROV-O](https://www.w3.org/TR/prov-overview/) | Explaining source profiles, evidence records, assertion states, and provenance. |

Progressive-disclosure rule:

- Start with the generated Solution Design Brief.
- Open the underlying ontology records only when reviewers ask what supports a section.
- Open source profiles and provenance when reviewers ask where a fact came from.
- Open validation/research anchors only when reviewers ask why a finding exists or how it would be automated.
