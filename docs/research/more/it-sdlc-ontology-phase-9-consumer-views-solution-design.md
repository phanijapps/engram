---
type: ontology-phase
title: Phase 9 - Consumer Views and Solution Design Enablement
slug: it-sdlc-ontology-phase-9-consumer-views-solution-design
project: unified-knowledge-base
phase: 9
status: first-view-accepted
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - it-sdlc-ontology
  - solution-design
  - consumer-views
  - application-landscape
---

## Purpose

Phase 9 defines how the ontology becomes useful to humans and downstream knowledge consumers.

The ontology should not only model the enterprise IT and SDLC landscape. It should help people do real work:

- Understand what a product, digital experience, software system, deployable unit, or interface contract does.
- See how a proposed solution fits into the current application landscape.
- Identify reusable assets, integration surfaces, data responsibilities, owners, constraints, standards, NFRs, controls, risks, and exceptions.
- Assemble solution-design review material from governed facts rather than manually rediscovering the landscape.
- Ground AI/RAG answers in reviewed ontology assertions and evidence.

This phase should answer:

> What views, packets, and human-facing experiences should the ontology produce so solution designers, architects, reviewers, and delivery teams can make better decisions?

## Design Principle

Views are projections, not another ontology.

The core ontology should own reusable meaning. Consumer views should select, group, filter, label, and explain that meaning for a specific job.

Do not put UI concepts, report sections, or tool-specific screen layouts into the core ontology unless they represent durable enterprise semantics.

## Context Requirements For Views

Every consumer view should declare its context requirements explicitly. This keeps AI and human-facing outputs from becoming generic data dumps.

| View Context Field | Meaning |
|--------------------|---------|
| contextAnchor | Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, or other entity that starts retrieval. |
| targetGate | Gate or workflow the view supports. |
| requiredContext | Facts that must be retrieved for the view to be useful. |
| hiddenByDefault | Context that remains available for drill-down but should not appear in the first screen or primary packet. |
| drillDownContext | Evidence, source assertions, source records, standards, rules, and research anchors available on demand. |
| allowedFactStates | Fact states that may appear in the main answer versus caveats or findings. |
| freshnessChecks | Source or fact-age warnings that should be shown before using stale context. |
| elicitationTriggers | Missing or ambiguous facts that should become human questions instead of invented answers. |

For the Solution Design Brief View, the primary context anchor should be a Product, DigitalExperience, or SoftwareSystem. The view should retrieve the scoped graph around that anchor, the target gate profile, applicable standards/NFRs/controls, required attributes, evidence, and unresolved decisions. Raw source records, low-confidence inferred facts, and deep policy text should stay in progressive disclosure unless they are directly needed for a decision.

## Primary Consumers

| Consumer | Needs From The Ontology |
|----------|--------------------------|
| Product owner | What product or experience is in scope, which outcomes it supports, and which teams own it. |
| Solution architect | What systems, deployable units, interfaces, data, constraints, and NFRs must be considered in the design. |
| Domain architect | How the solution aligns to business capabilities, domains, processes, and target architecture. |
| Engineering lead | Which repositories, deployable units, interface contracts, delivery dependencies, and build evidence are relevant. |
| Integration/API owner | Which APIs, events, files, queues, streams, and contracts exist or are proposed. |
| Data owner/steward | Which data entities/assets are touched, mastered, exchanged, classified, or governed. |
| Security/risk/control reviewer | Which standards, controls, policies, risks, exceptions, and evidence apply. |
| Operations/support owner | Which runtime services, environments, support models, incidents, and SLOs are relevant. |
| Portfolio/EA reviewer | What reuse, rationalization, overlap, lifecycle, and strategic-fit questions are raised. |
| AI/RAG consumer | Which facts can be cited, which are candidate/inferred/disputed, and which evidence supports an answer. |

## Accepted First Pilot View

Decision status: accepted on 2026-06-29.

The first consumer-facing pilot output is the **Solution Design Brief View**.

Reason:

- It forces the ontology to support a real approval conversation.
- It brings together application landscape, product/system composition, integration surfaces, required attributes, standards, NFRs, controls, risks, exceptions, evidence, and open decisions.
- It keeps the pilot grounded in a useful human artifact rather than only validating the model internally.

Supporting views for the first pilot:

- Application Landscape Context View.
- Product/System Composition View.
- Integration Surface View.
- Required Attributes and Gate Readiness View.
- Standards, NFR, Controls, and Regulation Applicability View.
- Evidence and Provenance View.

## View Catalog

### 1. Solution Design Brief View

Purpose:

- Provide the concise design context for an initiative, feature, epic, product change, or system change.

Should show:

- Business capability and outcome.
- Product or digital experience in scope.
- Software systems affected.
- New or changed deployable units.
- New or changed interface contracts.
- Key data entities/assets.
- Required solution-design attributes.
- Applicable standards, constraints, NFRs, controls, risks, and exceptions.
- Open decisions and validation findings.

Questions answered:

- What is being designed?
- What already exists?
- What must be reused, changed, integrated, or governed?
- What facts are missing before approval?

### 2. Application Landscape Context View

Purpose:

- Show how the proposed work fits into the broader application landscape.

Should show:

- Product and system boundaries.
- Related systems and platforms.
- Upstream/downstream dependencies.
- Ownership and support teams.
- Lifecycle posture.
- Strategic posture, if available.
- Known overlap or reuse candidates.

Questions answered:

- Where does this fit?
- What depends on it?
- What does it depend on?
- Is there an existing capability, system, deployable unit, or interface contract that should be reused?

### 3. Product/System Composition View

Purpose:

- Make the product-to-runtime stack clear to humans.

Should show:

```text
Product
  -> DigitalExperience
    -> SoftwareSystem
      -> DeployableUnit
        -> InterfaceContract
          -> RuntimeService
```

Should include:

- Micro-frontends.
- BFFs.
- Backend services.
- Agentic apps or agent services.
- Batch jobs.
- Data pipelines.
- Workflow workers.
- Runtime workloads, where needed.

Questions answered:

- Is this a product, experience, system, deployable unit, contract, or runtime service?
- Which parts are independently built, versioned, deployed, scaled, or operated?
- Which parts are user-facing versus internal?

### 4. Integration Surface View

Purpose:

- Make system interaction explicit before solution design approval.

Should show:

- APIs.
- Events.
- Queues.
- Streams.
- File transfers.
- Batch jobs.
- SaaS connectors.
- UI composition contracts.
- Agent/tool contracts.
- Providers and consumers.
- Contract status and evidence.
- Data entities exchanged.
- Security and access model, where known.

Questions answered:

- How do systems interact?
- Which contracts already exist?
- Which contracts are proposed or missing?
- What data crosses boundaries?
- Who owns provider and consumer obligations?

### 5. Required Attributes and Gate Readiness View

Purpose:

- Make solution-design required attributes visible and gate-aware.

Should show attributes by category:

| Category | Example Attributes |
|----------|--------------------|
| Ownership | product owner, system owner, technical owner, support owner, data owner |
| Lifecycle | lifecycle state, strategic posture, target state, decommission plan |
| Business | capability, business process, journey, outcome, criticality |
| Architecture | pattern, hosting model, platform, dependencies, target architecture fit |
| Integration | interface type, provider, consumer, contract, protocol, volume, latency |
| Data | data entities, classification, residency, lineage, retention, mastership |
| Security | authentication, authorization, secrets, encryption, threat model status |
| Resilience/NFR | availability, recoverability, scalability, performance, SLO/SLA |
| Delivery | work items, repositories, builds, tests, releases, deployment path |
| Operations | runtime service, environment, monitoring, alerting, support model |
| Governance | standards, controls, risks, exceptions, evidence, review status |

Questions answered:

- Which required attributes are complete?
- Which are missing?
- Which missing facts block the current gate?
- Which missing facts can move forward as planned evidence or open risk?

### 6. Standards, NFR, Controls, and Regulation Applicability View

Purpose:

- Show why a standard, NFR, control, policy, or regulation applies.

Should show:

- Normative source.
- Requirement, constraint, control, or quality attribute.
- Applicability rule.
- Target entity.
- Gate relevance.
- Required evidence.
- Exception status.
- Owner.

Questions answered:

- Why does this requirement apply?
- What evidence is needed?
- Is there an approved exception?
- Is this stable policy, a semi-stable applicability rule, or a living design fact?

### 7. Reuse and Dependency View

Purpose:

- Help solution designers avoid unnecessary duplication.

Should show:

- Candidate reusable products, systems, deployable units, data assets, and contracts.
- Functional fit.
- Non-functional fit.
- Ownership and support model.
- Integration cost.
- Known constraints.
- Adoption risks.
- Reuse recommendation status.

Questions answered:

- Can we reuse something instead of building new?
- What prevents reuse?
- Which dependencies create delivery or operational risk?

### 8. Delivery Traceability View

Purpose:

- Link design intent to delivery execution without hard-coding a specific SDLC tool.

Should show:

- Requirement.
- Work item.
- Repository.
- Build.
- Test evidence.
- Release.
- Deployment.
- Environment.
- Approval/evidence links.

Questions answered:

- Which delivery work implements this design?
- Which repository or deployable unit owns it?
- What evidence exists?
- What is still planned or missing?

### 9. Runtime and Operational Readiness View

Purpose:

- Show whether the designed solution can be operated.

Should show:

- Runtime services.
- Environments.
- Support teams.
- Monitoring/alerting.
- SLO/SLA.
- Incident/problem/change history, where relevant.
- Dependencies.
- Capacity or scaling assumptions.
- Disaster recovery and resilience attributes.

Questions answered:

- What will run in production?
- Who supports it?
- How will it be observed?
- What operational risks remain?

### 10. Evidence and Provenance View

Purpose:

- Make trust inspectable.

Should show:

- Source assertion.
- Source system.
- Evidence artifact.
- Assertion status.
- Review owner.
- Review date.
- Confidence.
- Conflicts.
- Promotion status.

Questions answered:

- Where did this fact come from?
- Who reviewed it?
- Is it authoritative, candidate, inferred, disputed, deprecated, or rejected?
- Can this fact be used at a gate?

### 11. Decisions and Exceptions View

Purpose:

- Preserve design judgment and controlled deviations.

Should show:

- Decision.
- Context.
- Alternatives considered.
- Outcome.
- Affected concepts, relationships, source profiles, or validation rules.
- Exception target.
- Exception reason.
- Approval owner.
- Expiration/review date.
- Compensating control or remediation plan.

Questions answered:

- What did we decide?
- Why did we decide it?
- What exceptions are active?
- When must they be reviewed?

### 12. AI/RAG Grounding View

Purpose:

- Define what AI systems are allowed to use and cite.

Should show:

- Query intent.
- Eligible fact states.
- Required citations.
- Confidence/risk labels.
- Source recency.
- Review status.
- Forbidden or advisory-only facts.
- Suggested follow-up questions.

Questions answered:

- Which facts can an AI assistant use as truth?
- Which facts require caveats?
- Which facts must be excluded from automated recommendations?

## Solution Design Packet Pattern

A solution design packet should be assembled from views. It should not become a new source of truth.

Recommended packet sections:

1. Design summary.
2. Product/system scope.
3. Application landscape context.
4. Product/system composition.
5. Integration surfaces.
6. Required attributes.
7. Standards, NFRs, controls, regulations, risks, and exceptions.
8. Reuse and dependency analysis.
9. Delivery traceability.
10. Operational readiness.
11. Evidence and provenance.
12. Open validation findings.
13. Decisions needed at the gate.

Each section should cite ontology-backed facts and distinguish:

- Reviewed facts.
- Authoritative facts.
- Candidate facts.
- Inferred facts.
- Disputed facts.
- Missing facts.
- Planned evidence.

## Progressive Disclosure Pattern

A consumer view should not expose the full graph by default.

Use progressive disclosure:

| Level | What The User Sees |
|-------|---------------------|
| Level 1 - Summary | Answer, status, owner, and key blockers. |
| Level 2 - Design facts | Relevant systems, units, contracts, data, controls, NFRs, and dependencies. |
| Level 3 - Evidence | Source assertions, evidence artifacts, review status, and confidence. |
| Level 4 - Source records | Tool/source-specific records and mapping details. |
| Level 5 - Rule/research anchor | Applicability rule, validation rule, standard, or external research anchor. |

This keeps reviews human-readable while still allowing deep inspection when a reviewer asks "why?"

## View Generation Rules

Consumer views should follow these rules:

- Start from the Phase 5 context operation policy.
- Always show target gate and validation profile.
- Always show fact state for non-authoritative facts.
- Always show provenance for facts used in a decision.
- Include a context trace for AI-generated packets.
- Do not promote inferred facts into design truth without review.
- Do not hide conflicts; classify them.
- Do not flatten all attributes into a single application profile.
- Do not hard-code tool fields into the view model.
- Do not require every source lane to be complete before producing a useful view.
- Convert missing required context into elicitation prompts or validation findings.
- Prefer concise packets for gate review and deeper drill-down for architects/stewards.

## Pilot Example

Example pilot slice:

```text
Product: Participant Retirement Advice
DigitalExperience: Participant web advice experience
SoftwareSystem: Advice orchestration system
DeployableUnits:
  - advice-goal-planning micro-frontend
  - advice-web BFF
  - advice-orchestration service
  - recommendation-agent service
  - advice-event-publisher
InterfaceContracts:
  - Advice Plan API
  - Participant Profile API
  - Advice Recommendation Event
  - Agent tool contract for plan explanation
DataEntities:
  - Participant
  - Retirement Goal
  - Advice Recommendation
  - Account Balance
Constraints/NFRs/Controls:
  - PII classification
  - authentication and authorization standard
  - availability target
  - model/agent review control, if agentic behavior is in scope
```

The Solution Design Brief View should be able to show:

- The experience is composed of a micro-frontend, BFF, backend service, event publisher, and agent service.
- The Advice Plan API and Advice Recommendation Event are the key integration surfaces.
- Participant and Account Balance data have classification and access implications.
- The recommendation-agent service is a deployable unit with an agentic extension, not a generic "application service."
- Solution Design Approval is blocked until data classification, interface ownership, NFR targets, and control applicability are reviewed.
- Build Readiness can accept planned evidence for test automation, CI build evidence, and repository linkage.
- Deploy to Production requires stronger evidence for runtime monitoring, operational support, release approval, and control evidence.

## What Not To Do

Avoid these traps:

- Do not expose raw ontology triples or graph structures as the default experience.
- Do not make views redefine core concepts.
- Do not let each enterprise tool invent its own view semantics.
- Do not treat "Application" as the one universal screen if it hides product, system, deployable, contract, and runtime distinctions.
- Do not use AI/RAG answers unless the answer can cite fact state and evidence.
- Do not treat all validation gaps as deploy blockers; classify gaps by gate.

## Phase 9 Deliverables

The reviewed output of this phase should be:

- Consumer/persona list.
- View catalog.
- View-to-concept mapping.
- Solution design packet template.
- Required attribute catalog.
- Progressive disclosure rules.
- View generation rules.
- Pilot example view set.
- AI/RAG grounding contract.
- Review checklist for human usability.

## Review Questions

- Which consumer view should be created first for the pilot?
- What is the minimum solution design packet for the Solution Design Approval gate?
- Which required attributes must be visible on the first screen?
- Which attributes should be drill-down only?
- Which facts can AI/RAG use as authoritative?
- Which source facts require human review before appearing in a packet?
- How should views show disagreement between source systems?
- Which view would make this useful enough for stakeholders to adopt?

## Research Anchors for Progressive Disclosure

Use these anchors when explaining consumer views and solution-design enablement.

| Anchor | Use When |
|--------|----------|
| [ISO/IEC/IEEE 42010](https://www.iso.org/standard/74393.html) | Explaining architecture views, viewpoints, stakeholders, and concerns. |
| [C4 Model](https://c4model.com/) | Explaining human-readable product/system/container/component-style views. |
| [ArchiMate](https://www.opengroup.org/archimate-forum/archimate-overview) | Explaining enterprise architecture viewpoints across business, application, technology, strategy, and implementation. |
| [Backstage System Model](https://backstage.io/docs/features/software-catalog/system-model/) | Explaining catalog-backed views of systems, components, APIs, resources, owners, and domains. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining API interface contract views. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining event/message/channel contract views. |
| [SHACL](https://www.w3.org/TR/shacl/) | Explaining view validation, required attributes, and gate findings. |
| [PROV-O](https://www.w3.org/TR/prov-overview/) | Explaining evidence, provenance, assertion states, and AI/RAG citation requirements. |

Progressive-disclosure rule:

- Start with the human job and the target gate.
- Bring in architecture-view anchors when discussing view design.
- Bring in catalog/contract anchors when discussing application landscape or integration-surface views.
- Bring in SHACL/PROV-O when discussing required attributes, validation findings, evidence, trust, or AI/RAG grounding.
