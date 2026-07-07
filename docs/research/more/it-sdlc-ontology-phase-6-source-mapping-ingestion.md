---
type: ontology-phase
title: Phase 6 - Source Mapping and Progressive Ingestion
slug: it-sdlc-ontology-phase-6-source-mapping-ingestion
project: unified-knowledge-base
phase: 6
status: pilot-recommendation-accepted
provenance: ai-assisted
created: 2026-06-27
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - ontology
  - source-mapping
  - ingestion
  - provenance
---

## Purpose

Phase 6 defines how real enterprise tools, repositories, documents, and runtime systems map into the canonical ontology.

The goal is to make the ontology operational without letting any one source system become the ontology. The canonical model owns shared meaning. Source systems own specific records, evidence, and operational state within declared authority boundaries.

This phase should answer:

> Which systems provide which facts, how do those facts map to canonical concepts, which source wins when there is conflict, and how do we preserve provenance and review status?

Phase 6 consumes the Phase 5 context operation policy. Source profiles must explain not only what a source can ingest, but also what kind of context it provides, how current it must be, when it should be retrieved, and when the ontology should ask a person instead.

## Design Principles

- Source systems are mapped through profiles, not copied directly into the core.
- Every imported fact should preserve source system, source record ID, timestamp, and authority status.
- Mappings should be explicit, versioned, testable, and owned.
- The ontology should tolerate incomplete data while making gaps visible.
- Entity resolution should be conservative; do not merge records just because names are similar.
- AI may propose mappings and matches, but governed sources or human review must approve authoritative mappings.

## Relationship To Context Operations

Source mapping should be driven by context needs, not by source availability alone.

Each source profile should declare:

- Which context roles the source can support.
- Which attributes or relationships it is authoritative for.
- Whether records should be retrieved as stable context, scoped graph context, rule-triggered context, live context, or evidence drill-down.
- Whether the source should be synced/indexed, linked-only, live/federated, manually elicited, or excluded from default retrieval.
- Where the source's context attaches: source record, document/object metadata, catalog definition, graph/index, trace, or compiled packet.
- How stale the source can be before a solution-design or deployment gate should warn.
- Which missing facts should be elicited from people instead of imported from the source.
- Which facts should remain candidate context until reviewed.
- How accepted/rejected context from reviews should influence source-correctness scoring or future proposals.

## Source Profile Pattern

Each source system gets a source profile.

```text
SourceProfile
  sourceSystem: ServiceNow CMDB
  profileOwner: Architecture Knowledge Team
  sourcePurpose: Application/service inventory and operational ownership
  contextRole: source fact and evidence
  attachmentLayer: source record and graph/index
  syncPattern: scheduled pull
  connectorMode: synced/indexed
  captureMode: source ingestion
  updateMode: scheduled sync with conflict review
  retrievalMode: scoped graph retrieval and evidence drill-down
  freshnessExpectation: reviewed every 30 days for design use
  elicitationPolicy: elicit ownership or boundary conflicts only
  authorityScope:
    - SoftwareSystem.lifecycleState
    - SoftwareSystem.criticality
    - RuntimeService.supportedBy
  mappedConcepts:
    - SoftwareSystem
    - RuntimeService
    - Team
    - Environment
  identityKeys:
    - sys_id
    - application_id
    - service_id
  reviewStatus: candidate
```

Profiles should include:

| Field | Meaning |
|-------|---------|
| sourceSystem | Tool, repository, dataset, or document source. |
| profileOwner | Team responsible for mapping quality. |
| contextRole | How the source contributes to AI-ready context: source fact, evidence, live signal, rule source, or candidate enrichment. |
| attachmentLayer | Where the context attaches: source record, document/object metadata, catalog definition, graph/index, trace, or compiled packet. |
| syncPattern | Manual, scheduled pull, event-driven, webhook, repository scan, API crawl, or batch import. |
| connectorMode | Synced/indexed, linked-only, live/federated, manually elicited, or excluded by default. |
| captureMode | How facts first enter the ontology from the source. |
| updateMode | How facts from the source stay current. |
| retrievalMode | How facts from the source should be selected for AI context. |
| freshnessExpectation | When source facts become stale for a given gate or consumer view. |
| elicitationPolicy | When to ask a human rather than rely on this source. |
| securityPolicy | Access control, classification, and redaction expectations for retrieved source facts. |
| authorityScope | Canonical concepts/attributes/relationships the source is authoritative for. |
| mappedConcepts | Canonical concepts created or enriched by the source. |
| identityKeys | Source fields used for identity resolution. |
| mappingRules | Field and relationship mappings into the ontology. |
| validationProfile | Rules applied during ingestion. |
| conflictPolicy | How conflicts are detected and resolved. |
| sourceFeedbackPolicy | How accepted/rejected context, reviewer corrections, and failed retrievals influence source-quality warnings or candidate proposals. |
| reviewStatus | Draft, reviewed, approved, disputed, deprecated. |

## Candidate Source Systems

| Source | Likely Canonical Concepts | Likely Authority |
|--------|---------------------------|------------------|
| Enterprise architecture repository, such as Ardoq or LeanIX | Product, SoftwareSystem, InterfaceContract, BusinessCapability, TechnologyComponent, Standard | Architecture relationships, system boundaries, standards fit, target disposition |
| CMDB / ServiceNow CSDM | SoftwareSystem, RuntimeService, Environment, Team, Incident, Change, Problem | Operational ownership, lifecycle, criticality, support, incident/change records |
| Portfolio tooling / Jira Align | Product, Outcome, Requirement, WorkItem, Release | Portfolio hierarchy, epics/features, roadmap intent, value-stream planning |
| Jira / Azure DevOps | WorkItem, Requirement, Defect, Change, TestCase | Delivery work status, defects, implementation traceability |
| GitHub / GitLab / Bitbucket | Repository, DeployableUnit, PullRequest, Commit, Artifact | Source ownership, implementation location, change evidence |
| CI/CD tooling | Build, Artifact, Release, Deployment, Environment | Build/release/deployment evidence |
| API catalog / gateway | InterfaceContract, API, IntegrationSurface, DataContract, Producer, Consumer | API inventory, owners, consumers, endpoints, policies |
| Event broker / schema registry | InterfaceContract, EventStream, Topic, Schema, DataContract | Event topics, message schemas, producers, consumers |
| Data catalog / lineage tooling | DataEntity, DataAsset, DataClassification, DataContract, DataOwner | Data ownership, classification, lineage, retention/residency metadata |
| Observability platforms | RuntimeService, Metric, Trace, Log, Alert, Dependency, SLO | Runtime telemetry, live dependencies, SLO evidence |
| GRC / control tooling | Control, Risk, Evidence, Exception, Policy, Finding | Control scope, evidence, exceptions, risk treatment |
| Documentation repositories | Standard, ArchitectureDecision, Evidence, InterfaceContract, Runbook | Human-authored decisions, runbooks, patterns, standards, design artifacts |
| Document/object metadata, evidence annotations, storage/catalog annotations | KnowledgeSource, Evidence, SourceAssertion, DataAsset, Standard | Business context, usage rules, source links, freshness, access classification, and evidence provenance |
| Agent/tool/action registry, MCP servers, automation catalogs | Agent, Tool, Action, InterfaceContract, IntegrationSurface, Evidence | Approved execution surfaces, contracts, scopes, owners, constraints |
| AI/RAG knowledge stores | KnowledgeSource, DataAsset, Evidence, OntologyConcept | Retrieval source metadata, chunk provenance, usage constraints |

## Mapping to the Accepted Stack

| Canonical Concept | Typical Source Records | Mapping Notes |
|-------------------|------------------------|---------------|
| Product | Portfolio product, value stream product, Jira Align solution/product, business offering | Do not confuse Product with SoftwareSystem. A product may use many systems. |
| DigitalExperience | Channel, journey, portal, mobile app, web experience, advisor desktop, agent chat experience | May be sourced from architecture repository, product docs, or solution design artifacts. |
| SoftwareSystem | Application, business application, system, bounded system, architecture component | Preferred canonical replacement for overloaded "application". |
| DeployableUnit | Micro-frontend, BFF, backend service, worker, batch job, agentic app, data pipeline, repo component | Usually discovered from repos, CI/CD, Backstage-like catalogs, or architecture docs. |
| InterfaceContract | API definition, event schema, file feed spec, UI fragment contract, agent capability, tool contract | Should link to machine-readable contract where possible. |
| RuntimeService | Running service, workload, process, application instance, function, deployment target | Often sourced from CMDB, observability, cloud inventory, or platform APIs. |
| AgentToolAction | Approved agent, tool, MCP resource, automation action, workflow action, or runtime capability | Should link to owner, contract, scope, sensitive-action flag, approval state, and allowed invocation conditions. |

## Record Authority Matrix

The first pilot should define authority at the attribute/relationship level, not just at the entity level.

| Fact | Preferred Authority | Secondary Source | Conflict Rule |
|------|---------------------|------------------|---------------|
| Product owner | Portfolio/product tooling | Architecture repository | Portfolio wins unless marked stale. |
| Business capability support | Architecture repository | Solution design docs | Reviewed architecture relationship wins. |
| SoftwareSystem lifecycle state | CMDB or architecture repository | Portfolio tooling | Declared enterprise source wins. |
| Strategic posture | Architecture governance repository | CMDB | Architecture governance wins. |
| DeployableUnit repository | Source catalog / Git provider | Architecture docs | Repository metadata wins; docs can propose. |
| InterfaceContract definition | API catalog / schema registry | Repository | Contract registry wins for published contracts. |
| Interface consumers | API gateway / telemetry / contract registry | Architecture docs | Runtime evidence may propose; contract registry approves. |
| Deployment timestamp | CI/CD tooling | Change record | CI/CD wins for actual timestamp. |
| Production support team | CMDB / ITSM | Repository ownership | CMDB wins for production support. |
| Runtime dependency | Observability telemetry | Architecture docs | Telemetry proposes; reviewed architecture can confirm or suppress. |
| Control applicability | GRC tooling | Architecture review | GRC wins for formal control scope. |
| Control evidence | GRC tooling / CI/CD evidence store | Documents | Evidence source must preserve provenance. |
| Approved tool/action scope | Agent/tool registry or platform owner | Repo manifest / design doc | Registry wins; design docs can propose missing action context. |
| Trace-derived relationship | Human-reviewed trace summary | Raw trace / AI inference | Trace can propose only; review must approve before use for gate decisions. |

## Entity Resolution Strategy

Entity resolution should happen in layers.

1. Deterministic match: same canonical ID, source-system cross-reference, or registered alias.
2. Scoped key match: same source ID inside known system namespace.
3. Curated alias match: approved alias table links records across tools.
4. Candidate match: name/domain/team similarity proposes a match for review.
5. Rejected match: explicitly record known false matches to prevent repeated suggestions.

Do not merge records automatically when:

- Names are similar but owners differ.
- One source uses product names and another uses software-system names.
- Runtime service names are environment-specific variants.
- Repositories contain multiple deployable units.
- A vendor/COTS platform hosts multiple tenant-specific systems.

## Source Assertion Model

Each imported fact should be represented as an assertion.

```text
SourceAssertion
  subject: SoftwareSystem/advice-orchestration
  predicate: ownedBy
  object: Team/advice-platform-team
  sourceSystem: Ardoq
  sourceRecordId: app-12345
  assertedAt: 2026-06-27T10:00:00Z
  authorityLevel: authoritative
  confidence: high
  reviewStatus: approved
```

This separates the canonical relationship from its supporting evidence and allows conflicting facts to be reviewed instead of overwritten.

## Conceptual Progressive Ingestion Model

This phase includes a conceptual ingestion progression. It should not prescribe the physical implementation. It does not require a specific graph database, pipeline product, message bus, workflow engine, API gateway, catalog, vector store, or integration pattern.

The goal is to define how knowledge becomes trusted over time.

| Stage | Name | Purpose | Output | Trust Level |
|-------|------|---------|--------|-------------|
| 0 | Frame | Identify the pilot scope, competency questions, target gates, and source candidates. | Ingestion scope and known unknowns. | None yet |
| 1 | Seed | Manually create a small canonical example from known-good human review. | Seed Product, SoftwareSystem, DeployableUnit, InterfaceContract, owner, and gate facts. | Reviewed/manual |
| 2 | Map | Define source profiles, field mappings, relationship mappings, identity keys, and authority scope. | Mapping specifications and sample records. | Candidate |
| 3 | Import | Bring source facts into a staging area as source assertions with provenance. | SourceAssertion records, not yet canonical truth. | Imported |
| 4 | Normalize | Map source records to canonical concepts and relationships. | Candidate canonical entities and relationships. | Candidate |
| 5 | Resolve | Match identities across sources, detect duplicates, conflicts, and false matches. | Resolved entities, unresolved conflicts, rejected matches. | Candidate/reviewed |
| 6 | Validate | Apply gate-aware validation profiles and source-quality checks. | Missing facts, invalid mappings, open risks, evidence gaps. | Reviewed for quality |
| 7 | Curate | Human or steward review approves, rejects, suppresses, or annotates candidate facts. | Approved facts, disputed facts, exceptions, remediation tasks. | Reviewed/approved |
| 8 | Promote | Promote approved facts into the governed ontology view. | Governed graph/view for solution design and gate review. | Authoritative where authority exists |
| 9 | Monitor | Track source drift, stale facts, new conflicts, validation failures, usage feedback, and trace/correction signals. | Drift reports, refresh backlog, ontology change proposals. | Living |
| 10 | Score | Track which sources, mappings, and retrieval paths produced accepted or rejected context. | Source-quality warnings and proposal confidence adjustments. | Advisory |
| 11 | Propose | Convert approved traces, reviewer corrections, failed retrievals, source-correctness signals, and extraction results into candidate wiki, manifest, or graph updates. | Candidate context updates and graph proposals. | Candidate |

This progression is conceptual. A future implementation could realize it with many different technologies. The ontology only needs the conceptual states and rules:

- source fact
- normalized fact
- candidate fact
- inferred fact
- reviewed fact
- authoritative fact
- disputed fact
- deprecated fact
- rejected fact

## Conceptual Ingestion States

| State | Meaning | Can Support Solution Design? | Can Support Gate Approval? |
|-------|---------|------------------------------|----------------------------|
| Source fact | A fact as represented by a source system. | Yes, as context. | No, unless source is authoritative and mapped. |
| Imported fact | A source fact copied into the ingestion scope with provenance. | Yes, as context. | No. |
| Normalized fact | A source fact mapped to a canonical concept or relationship. | Yes. | Sometimes, if reviewed or from authoritative source. |
| Candidate fact | A normalized fact pending review or confidence checks. | Yes, with caveat. | No, unless gate allows open risk. |
| Inferred fact | A fact proposed by telemetry, AI, document extraction, or heuristic matching. | Yes, as a hint. | No, until reviewed. |
| Proposed graph fact | A relationship or entity proposed from traces, documents, contracts, or generated-context feedback. | Yes, as candidate context with caveat. | No, until reviewed and promoted. |
| Compiled packet fact | A fact included in a task-specific context packet with source revision and retrieval profile. | Yes, for the packet's target task. | Only if underlying fact state and gate profile permit it. |
| Reviewed fact | A fact reviewed by a steward or approved workflow. | Yes. | Yes, if gate accepts reviewed authority. |
| Authoritative fact | A fact from the declared authority for that attribute or relationship. | Yes. | Yes. |
| Disputed fact | Conflicting facts exist or review disagrees. | Yes, as a risk. | No, unless exception exists. |
| Deprecated fact | A fact was valid but is being retired or replaced. | Yes, with warning. | Usually no for new designs. |
| Rejected fact | A proposed mapping or match was reviewed and rejected. | No, except as suppression memory. | No. |

## Conceptual Ingestion Lanes

Progressive ingestion should happen by evidence lane, not by trying to ingest the whole enterprise at once.

| Lane | What It Adds | Typical First Questions |
|------|--------------|-------------------------|
| Landscape lane | Products, software systems, experiences, deployable units, owners, lifecycle, strategic posture. | What exists, what does it do, who owns it? |
| Integration lane | Interface contracts, APIs, events, files, data contracts, producers, consumers. | How does it connect, and what does it exchange? |
| Delivery lane | Requirements, work items, repositories, builds, releases, deployments. | What changed, where is the code, what was delivered? |
| Runtime lane | Runtime services, environments, dependencies, telemetry, SLOs, incidents. | What is running, how is it behaving, what is impacted? |
| Governance lane | Standards, constraints, NFRs, controls, risks, evidence, exceptions. | What applies, what evidence exists, what risks remain? |
| Action lane | Agents, tools, resources, automation actions, contracts, scopes, and invocation constraints. | What can be done, through which approved surface, and under what conditions? |
| Trace-learning lane | Reviewer corrections, failed retrievals, trace summaries, accepted/rejected proposals. | What context was missing or wrong, and what should be proposed for review? |
| Knowledge lane | Documents, decisions, runbooks, patterns, research anchors, AI/RAG metadata. | What human knowledge explains or justifies the facts? |

For the pilot, each lane can mature independently. For example, the landscape and integration lanes may be reviewed enough for solution design while the runtime lane remains partial until production evidence exists.

## Progressive Ingestion Maturity

| Maturity | Description | What It Enables |
|----------|-------------|-----------------|
| Level 0: Inventory seed | A small manually reviewed set of product/system/interface facts. | Conversation and review around a concrete example. |
| Level 1: Mapped sources | Source profiles exist with mappings, identity keys, and authority declarations. | Repeatable import from selected sources. |
| Level 2: Staged assertions | Source facts are imported as assertions with provenance and status. | Gap/conflict analysis without polluting canonical truth. |
| Level 3: Reviewed canonical view | Key facts are normalized, resolved, validated, and promoted. | Solution design gate support. |
| Level 4: Gate-aware evidence | Validation profiles connect facts to gates, planned evidence, actual evidence, and exceptions. | Build/release/deploy governance support. |
| Level 5: Living feedback loop | Drift, telemetry, incidents, source changes, and user feedback generate review tasks. | Living ontology operations and continuous improvement. |

The pilot should target Level 3 for solution design and selectively Level 4 where controls or production readiness are in scope.

## Ingestion Patterns

| Pattern | Use For | Notes |
|---------|---------|-------|
| Manual seed | Early pilot records, curated examples, authoritative exceptions | Fastest way to start; must be labeled manual. |
| Repository scan | `catalog-info.yaml`, manifests, OpenAPI/AsyncAPI files, deployment descriptors, SBOMs | Good for deployable units and contracts. |
| API pull | CMDB, portfolio, GRC, architecture repository, API catalog | Preferred for governed systems of record. |
| Event/webhook | CI/CD, repository changes, deployment events, contract publication | Good for keeping delivery evidence current. |
| Batch import | Legacy inventories, spreadsheets, exported architecture models | Useful but high validation burden. |
| Telemetry inference | Observed runtime dependencies, consumers, SLO evidence | Treat as inferred until reviewed. |
| Document extraction | Solution designs, architecture decisions, runbooks, standards | Useful for proposals and evidence; requires review. |

## Mapping Workflow

1. Select pilot product/system.
2. Identify source systems for that pilot.
3. Create source profiles for each source.
4. Define canonical concepts and relationships expected from each source.
5. Map source fields to canonical attributes.
6. Map source relationships to canonical relationships.
7. Declare record authority and conflict policy.
8. Run ingestion into a staging graph.
9. Run gate-aware validation profiles.
10. Review gaps, conflicts, duplicates, and inferred relationships.
11. Promote approved mappings and assertions.
12. Record mapping decisions and open questions.

## Pilot Mapping Priority

For the first pilot, map the smallest set that can answer solution-design questions:

Decision status: accepted on 2026-06-27.

Pilot source strategy:

> Start with one architecture/source-of-truth tool, one SDLC tool, one repository/CI source, one API/integration source if available, and one GRC/control source if controls are in scope. Expand only after the mappings prove useful.

1. Product or candidate product.
2. Business capabilities/processes/outcomes.
3. Software systems involved.
4. Major deployable units.
5. Key interface contracts and integration surfaces.
6. Data entities exchanged or mastered.
7. Owners and support teams.
8. Required solution-design attributes.
9. Risks, controls, standards, and exceptions.
10. Deployment/runtime facts only where needed for production or impact analysis.

Avoid starting with every source system. Start with enough to prove that the ontology can support a real solution-design review.

## Mapping Quality Checks

| Check | Why It Matters |
|-------|----------------|
| Every mapped field has a canonical target or is intentionally ignored. | Prevents accidental data loss or uncontrolled schema growth. |
| Every authoritative field has one declared authority. | Prevents tool conflicts. |
| Every imported entity has source provenance. | Supports audit and troubleshooting. |
| Every inferred relationship has confidence and review status. | Prevents telemetry or AI suggestions from becoming false truth. |
| Every source profile has an owner. | Keeps mappings alive as tools change. |
| Every mapping has sample records. | Makes review concrete. |

## Phase 6 Deliverables

The reviewed output of this phase should be:

- Source system inventory for the pilot.
- Source profile template.
- Initial source profiles for pilot systems.
- Context operation policy for each pilot source profile.
- Record authority matrix.
- Entity resolution rules.
- Source assertion/provenance model.
- Conceptual progressive ingestion model.
- Conceptual ingestion states.
- Conceptual ingestion lanes.
- Target ingestion maturity level for the pilot.
- Ingestion pattern decision for each source.
- Pilot mapping priority list.
- Mapping quality checks.

## Review Questions

- Which source systems are in scope for the first pilot?
- Which source system is authoritative for product/system ownership?
- Which source system is authoritative for software-system lifecycle and strategic posture?
- Which source system is authoritative for interface contracts?
- Which source system is authoritative for control applicability and evidence?
- Which records should be manually seeded before automation?
- Which ingestion lanes are needed for the first pilot: landscape, integration, delivery, runtime, governance, knowledge?
- What maturity level is required for solution design approval?
- Which facts may remain candidate/inferred at solution design, and which must be reviewed or authoritative?
- Which inferred relationships require human review before promotion?

## Research Anchors for Progressive Disclosure

Use these anchors when explaining mapping or ingestion choices.

| Anchor | Use When |
|--------|----------|
| [PROV-O](https://www.w3.org/TR/prov-overview/) | Explaining source assertions, provenance, evidence, and who asserted what. |
| [DCAT](https://www.w3.org/TR/vocab-dcat-3/) | Explaining catalog metadata for datasets, services, distributions, and knowledge assets. |
| [Backstage System Model](https://backstage.io/docs/features/software-catalog/system-model/) | Explaining developer-catalog source profiles for systems, components, APIs, resources, owners, and domains. |
| [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) | Explaining ingestion of HTTP API contract definitions. |
| [AsyncAPI Specification](https://www.asyncapi.com/docs/reference/specification/v3.0.0) | Explaining ingestion of event/message/channel contract definitions. |
| [OpenTelemetry](https://opentelemetry.io/docs/concepts/) | Explaining inferred runtime dependencies and telemetry-derived evidence. |
| [SPDX](https://spdx.dev/) and [CycloneDX](https://cyclonedx.org/) | Explaining SBOM, package, dependency, license, and vulnerability evidence ingestion. |

Progressive-disclosure rule:

- Start with the source profile and authority question.
- Bring in provenance references when discussing trust, auditability, or conflict resolution.
- Bring in contract/catalog references when discussing API, event, repository, or developer-portal ingestion.
