---
type: ontology-roadmap
title: IT SDLC Ontology Feature Roadmap
slug: it-sdlc-ontology-feature-roadmap
project: unified-knowledge-base
status: drafted
provenance: ai-assisted
created: 2026-06-30
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - it-sdlc-ontology
  - feature-roadmap
  - capability-sequencing
---

## Purpose

This artifact translates the ontology phases into product, software, data, infrastructure, and platform capabilities.

The phase artifacts define the semantic model, context policy, source mapping, validation, governance, views, and pilot. This roadmap defines the features needed to make those artifacts operational, including ingestion pipelines, APIs, UIs, stores, indexes, workflow services, and operations tooling.

It should answer:

> What capabilities do we need to build or enable, in what order, so the ontology can become a living enterprise context-selection system for solution design?

## Placement Decision

This roadmap is not a numbered phase.

It should live beside the phase artifacts because it cuts across them:

- Phase 0 to Phase 4 define the ontology intent and semantic backbone.
- Phase 5 defines context selection policy.
- Phase 6 defines source mapping and progressive ingestion.
- Phase 7 and Phase 8 define validation and governance.
- Phase 9 to Phase 12 define views, specification, worked instance, and pilot execution.

The feature roadmap converts those phases into sequenced delivery work.

## Capability Map

| Capability Group | What It Enables | Primary Phase Inputs |
|------------------|-----------------|----------------------|
| Ontology authoring and semantic governance | Create, review, version, and publish the core ontology, controlled vocabularies, and federated extensions. | Phases 2, 3, 4, 8 |
| Source profiles and progressive ingestion | Describe sources, map fields, ingest source assertions, preserve provenance, and move facts through candidate/reviewed/authoritative states. | Phases 5, 6, 8 |
| Identity, resolution, and provenance | Resolve products/systems/contracts/data across tools without unsafe merges, and explain where each fact came from. | Phases 4, 6, 7 |
| Context attachment and semantic enrichment | Attach business context, usage rules, definitions, lineage, source-object metadata, and evidence context near the source/catalog layer. | Phases 5, 6, 8 |
| Context selection and retrieval | Select the right context for an AI or human task based on gate, anchor, authority, freshness, security, and evidence needs. | Phases 5, 6, 9, 10 |
| Agent, tool, and action governance | Govern which agents, tools, actions, resources, contracts, scopes, and invocation constraints are available for a task. | Phases 5, 7, 8, 9 |
| Gate validation and applicability | Validate required attributes, standards, NFRs, controls, evidence, risks, and exceptions by lifecycle gate. | Phases 5, 7, 8, 10 |
| Consumer views and solution design experience | Generate useful packets such as the Solution Design Brief with progressive disclosure and context traceability. | Phases 1, 9, 10, 11 |
| Human elicitation and review workflow | Ask people for missing intent, ownership, applicability, risk, and exception context when no authoritative source exists. | Phases 5, 7, 9, 11, 12 |
| Trace-driven context improvement | Use review corrections, agent traces, failed retrievals, and design decisions to propose context updates and graph changes. | Phases 5, 6, 8, 12 |
| Security, access, and trust | Apply classification, redaction, access control, auditability, and fact-state boundaries before retrieval or display. | Phases 5, 7, 8, 9 |
| Pilot operations and measurement | Run the first slice, measure usefulness, capture feedback, and decide whether to expand. | Phases 10, 11, 12 |

## Implementation Build Surfaces

The capability map above describes what the enterprise needs. This section lists the software and platform features that would actually be built or enabled.

The roadmap remains vendor-neutral. Terms such as graph store, vector store, relational database, and object store describe required platform capabilities, not product decisions.

### User Interfaces And Human Workflows

| Feature | What To Build | Sequence |
|---------|---------------|----------|
| Ontology steward console | UI for editing classes, relationships, properties, controlled vocabularies, research anchors, and release notes. | Foundation |
| Source profile and mapping workbench | UI for source owners to define source profiles, sample records, mapping rules, authority scope, freshness, and context policy. | Foundation |
| Mapping sample review UI | Side-by-side view of source records, proposed canonical records, field mappings, conflicts, and reviewer comments. | Foundation |
| Ingestion run monitor | UI showing ingestion runs, source freshness, failed records, quarantined facts, mapping drift, and retry status. | Pilot MVP |
| Context attachment workbench | UI for reviewing source-object annotations, document metadata, evidence metadata, business definitions, usage rules, and catalog semantic enrichment. | Scale |
| Verified examples workbench | UI for reviewing semantic examples, query patterns, expected answers, and grounding examples used to calibrate retrieval and generation. | Scale |
| Review queue and approval inbox | Workflow UI for candidate facts, conflicts, exceptions, applicability decisions, and fact promotion. | Pilot MVP |
| Solution Design Brief workspace | Human-facing workspace for generated briefs, validation findings, context trace, decisions needed, and reviewer disposition. | Pilot MVP |
| Context trace and provenance viewer | Drill-down UI showing included facts, excluded facts, source records, evidence, freshness, and fact state. | Pilot MVP |
| Agent/tool/action registry console | UI for approved agents, tools, MCP resources, actions, contracts, scopes, owners, sensitive-action flags, and lifecycle state. | Scale |
| Learned graph proposal queue | UI for reviewing candidate entities, relationships, wiki updates, and manifest updates proposed from traces or extraction. | Scale |
| Gate validation dashboard | View of required attributes, blocking/advisory findings, planned evidence, actual evidence, risks, exceptions, and gate posture. | Pilot MVP |
| Evidence and exception workspace | UI to attach evidence, request evidence, accept risks, approve exceptions, and track expiry/review dates. | Pilot MVP |
| Pilot cockpit | UI for pilot scope, anchor entity, source lanes, reviewers, open questions, success metrics, and expansion readiness. | Pilot MVP |
| Operating dashboard | UI for ontology health, source freshness, validation posture, context-selection performance, review backlog, and adoption. | Scale |
| Admin and access console | UI for roles, permissions, data classification rules, source access, and retrieval restrictions. | Scale |

### APIs And Application Services

| Feature | What To Build | Sequence |
|---------|---------------|----------|
| Ontology metadata API | Service API for classes, relationships, properties, vocabulary values, and version metadata. | Foundation |
| Source profile API | Service API for source profiles, authority scopes, mapping rules, source freshness, and context policy. | Foundation |
| Source assertion API | Service API for imported, seeded, inferred, and human-provided facts with fact state and provenance. | Pilot MVP |
| Identity resolution service | Service for canonical IDs, candidate matches, merge/split proposals, and identity confidence. | Pilot MVP |
| Provenance service | Service for source references, mapping rules, timestamps, evidence links, and lineage of fact promotion. | Pilot MVP |
| Graph traversal API | Service for scoped graph neighborhoods around products, systems, deployable units, contracts, data, controls, and evidence. | Pilot MVP |
| Context selector API | Service that selects facts based on task, gate, anchor, authority, freshness, security, and retrieval budget. | Pilot MVP |
| Context compilation service | Service that materializes task-specific context packets before generation, with included/excluded facts, source revisions, freshness, and provenance. | Scale |
| Grounding and answer evaluation service | Service that checks generated outputs against selected snippets, rows, fields, graph edges, citations, fact states, and verified examples. | Scale |
| Agent/tool registry API | Service API for agents, tools, resources, actions, contracts, owners, scopes, approval status, and invocation constraints. | Scale |
| Tool/resource selector service | Service that selects relevant allowed tools and resources for a task without bypassing approval or runtime policy. | Scale |
| Context packet builder | Service that assembles AI-ready and human-readable context packets with included/excluded facts and warnings. | Pilot MVP |
| Validation service | Service for gate-aware validation rules, severity, missing required attributes, and blocking/advisory findings. | Pilot MVP |
| Applicability rule service | Service that evaluates which standards, NFRs, controls, regulations, and evidence obligations apply. | Pilot MVP |
| Brief generation service | Service that generates Solution Design Brief drafts from context packets and view templates. | Pilot MVP |
| Elicitation workflow service | Service that turns missing context into structured questions and routes them to the right reviewer or owner. | Pilot MVP |
| Evidence service | Service for planned evidence, actual evidence, attachments, links, attestations, and evidence status by gate. | Pilot MVP |
| Access and classification policy service | Service that enforces permissions, classification, redaction, and sensitive-context suppression before retrieval. | Pilot MVP |
| Audit service | Service that records ontology changes, source ingestion, validation runs, context packets, approvals, and promotions. | Pilot MVP |
| Feedback and correction API | Service for user corrections, AI context feedback, candidate relationship feedback, and curation outcomes. | Scale |
| Trace learning service | Service that summarizes traces, detects missing context, proposes graph/wiki/manifest updates, and routes proposals to review. | Scale |
| Source-correctness scoring service | Service that learns which sources, mappings, relationships, and retrieval paths produce accepted answers or reviewer corrections. | Scale |

### Ingestion Pipelines And Connectors

| Feature | What To Build | Sequence |
|---------|---------------|----------|
| Connector framework | Standard pattern for source connectors, authentication, pagination, rate limits, retries, schema inspection, and run metadata. | Foundation |
| Manual seed importer | Controlled import path for pilot seed records before automation exists. | Foundation |
| Batch ingestion pipeline | Scheduled ingestion for portfolio, architecture, CMDB, catalog, data, GRC, and SDLC sources. | Pilot MVP |
| Event/webhook ingestion pipeline | Event-driven update path for source systems that can publish changes. | Scale |
| Federated live-query connector | On-demand retrieval path for volatile, sensitive, high-volume, or source-controlled data that should not be fully indexed. | Scale |
| Repository scanner | Pipeline that scans repositories for deployable units, ownership metadata, manifests, API specs, build files, and evidence. | Pilot MVP |
| Contract scanner | Pipeline for OpenAPI, AsyncAPI, event schemas, file specs, tool contracts, and interface metadata. | Pilot MVP |
| Agent/tool registry connector | Pipeline for approved internal agents, MCP servers, tools, action catalogs, and automation endpoints. | Scale |
| Document ingestion pipeline | Pipeline for architecture decisions, standards, runbooks, policies, design docs, and evidence documents. | Pilot MVP |
| Object/storage annotation sync | Pipeline for object-level annotations, source-object metadata, evidence metadata, and storage-layer context. | Scale |
| Document chunking and metadata enrichment | Split documents into retrievable chunks with source, section, security, freshness, and authority metadata. | Pilot MVP |
| Catalog semantic enrichment sync | Pipeline for business definitions, usage rules, runbooks, query examples, semantic views, lineage, object tags, and data-quality context. | Scale |
| Verified example ingestion | Pipeline for semantic-view examples, expected-answer examples, query patterns, approved snippets, and domain-specific retrieval examples. | Scale |
| Embedding pipeline | Create embeddings for document chunks, source descriptions, standards, and evidence where semantic retrieval is useful. | Pilot MVP |
| Data catalog sync | Connector for data entities, data assets, ownership, classification, lineage, retention, and residency metadata. | Pilot MVP |
| GRC/control sync | Connector for controls, policies, risks, exceptions, evidence requests, findings, and applicability metadata. | Pilot MVP |
| CI/CD evidence collector | Pipeline for builds, artifacts, releases, deployments, environments, test results, and deployment evidence. | Pilot MVP |
| Observability summarizer | Pipeline that summarizes runtime dependencies, SLOs, incidents, alerts, telemetry, and operational posture without overloading the graph. | Scale |
| Agent trace ingestion and summarization | Pipeline that captures redacted traces, failed retrievals, tool-selection outcomes, reviewer corrections, and generated-context feedback. | Scale |
| Source schema drift detector | Detect changes in source fields, enums, API responses, document shapes, or contract schemas that may break mappings. | Pilot MVP |
| Mapping test harness | Automated tests against sample records to prove mappings still produce expected canonical facts. | Foundation |
| Quarantine and dead-letter pipeline | Hold failed, conflicting, low-confidence, or policy-violating records for review instead of promoting them. | Pilot MVP |
| Fact promotion pipeline | Move facts from source assertion to candidate, reviewed, authoritative, disputed, or deprecated states through governed workflow. | Pilot MVP |

### Data Stores, Indexes, And Platform Infrastructure

| Feature | What It Stores Or Provides | Sequence |
|---------|----------------------------|----------|
| Graph store | Canonical ontology entities and relationships, scoped graph traversal, dependency paths, applicability paths, and impact analysis. | Foundation |
| Graph proposal workspace | Candidate entities, relationships, edge evidence, extraction provenance, reviewer disposition, and promotion history. | Scale |
| Relational database | Transactional records such as source profiles, workflow state, review assignments, gate profiles, users, permissions, and configuration. | Foundation |
| Object store | Raw source snapshots, evidence files, document originals, generated packets, export archives, and immutable ingestion artifacts. | Foundation |
| Vector store or vector index | Embeddings for document chunks, standards, evidence, design patterns, and semantic retrieval candidates. | Pilot MVP |
| Semantic cache | Reusable context snippets, repeated retrieval results, accepted generated answers, and cache invalidation metadata. | Scale |
| Search index | Keyword/faceted search over ontology records, source assertions, documents, evidence, and review artifacts. | Pilot MVP |
| Rules store | Versioned validation rules, applicability rules, gate profiles, severity overrides, and test cases. | Foundation |
| Audit log store | Append-only or tamper-evident history for source ingestion, context packets, approvals, exceptions, and ontology releases. | Pilot MVP |
| Trace/event store | Redacted agent traces, retrieval traces, tool-selection outcomes, reviewer corrections, and context-quality events. | Scale |
| Grounding evaluation store | Evaluation results for generated answers, selected snippets, missing citations, unsupported claims, rejected examples, and accepted examples. | Scale |
| Agent memory store | Governed, scoped memory for repeated agent tasks, reviewer preferences, accepted clarifications, and expiry-controlled interaction context. | Scale |
| Cache | Low-latency cache for ontology metadata, scoped graph neighborhoods, context packets, and source freshness summaries. | Scale |
| Queue or event bus | Asynchronous ingestion, fact promotion, validation runs, embedding jobs, review notifications, and source-change events. | Pilot MVP |
| Compute runtime for jobs | Runtime for scheduled ingestion, scans, validation, embedding, summarization, and export jobs. | Pilot MVP |
| API gateway | Managed access point for UI, services, source callbacks, and consumer integrations. | Pilot MVP |
| Secrets and key management | Secure storage and rotation for connector credentials, signing keys, encryption keys, and service credentials. | Foundation |
| IAM integration | Enterprise identity, role mapping, group-based access, service identity, and privileged operations. | Foundation |
| Backup, restore, and disaster recovery | Recovery strategy for graph, relational, object, vector, search, rules, and audit stores. | Scale |
| Retention and lifecycle management | Policies for source snapshots, evidence, generated packets, embeddings, audit logs, and deprecated facts. | Scale |

### AI, Retrieval, And Context Runtime

| Feature | What To Build | Sequence |
|---------|---------------|----------|
| Retrieval planner | Chooses graph, search, vector, live query, rule-triggered retrieval, evidence drill-down, or elicitation based on task. | Pilot MVP |
| Context compiler | Produces task-ready context artifacts for gates, briefs, impact analysis, and agent workflows before model invocation. | Scale |
| Tool/action planner | Chooses relevant allowed tools, resources, and actions for context-aware solution design without granting implicit execution. | Scale |
| Context budget manager | Limits retrieved context by relevance, gate, authority, freshness, sensitivity, and model/context-window budget. | Pilot MVP |
| Snippet and field grounding map | Maps generated claims to selected snippets, source rows, manifest fields, graph edges, verified examples, and evidence links. | Scale |
| Semantic cache policy | Governs when repeated context can be cached, reused, invalidated, suppressed, or forced to recompile. | Scale |
| Context redaction and suppression | Removes sensitive, stale, unauthorized, disputed, or irrelevant facts before AI use. | Pilot MVP |
| Prompt and packet template registry | Versioned templates for Solution Design Briefs, validation summaries, evidence summaries, and reviewer questions. | Pilot MVP |
| Citation/provenance linker | Links generated statements back to ontology facts, source assertions, evidence, and source records. | Pilot MVP |
| AI response evaluator | Checks generated briefs and answers for missing citations, unsupported claims, stale context, and hidden candidate facts. | Scale |
| Verified example evaluator | Tests retrieval and generation against approved examples and expected answers for known pilot/design scenarios. | Scale |
| Context learning loop | Converts evaluation failures, reviewer corrections, and trace summaries into candidate updates with evidence and confidence. | Scale |
| Source-quality learning loop | Adjusts source authority warnings and proposal confidence based on accepted/rejected context and reviewer corrections. | Scale |
| Model/provider adapter | Allows the retrieval and generation layer to work across approved enterprise AI providers without changing ontology semantics. | Scale |
| Human feedback capture | Captures reviewer corrections to retrieval choices, generated briefs, citations, and elicitation prompts. | Scale |

### DevOps, Testing, And Operations

| Feature | What To Build | Sequence |
|---------|---------------|----------|
| Infrastructure as code | Provision graph, relational, object, vector, search, queue, compute, API, IAM, and network resources repeatably. | Foundation |
| CI/CD for ontology services | Build, test, scan, deploy, and roll back APIs, ingestion jobs, UI, validation services, and context services. | Foundation |
| Synthetic pilot dataset | Safe test data for products, systems, contracts, controls, evidence, source assertions, and generated briefs. | Foundation |
| Golden-context regression tests | Ensure context selector changes do not silently alter approved Solution Design Brief outputs. | Pilot MVP |
| Golden-answer evaluation tests | Ensure unsupported claims, wrong snippets, stale context, and hidden candidate facts are detected. | Scale |
| Golden-trace regression tests | Ensure trace summarization and update proposal changes do not silently create unsafe or unsupported graph updates. | Scale |
| Mapping regression tests | Ensure source connector and mapping changes still produce expected canonical records. | Pilot MVP |
| Data quality checks | Monitor duplicates, missing required fields, stale facts, unresolved conflicts, invalid relationships, and orphaned evidence. | Pilot MVP |
| Observability and SLOs | Metrics, logs, traces, alerts, ingestion SLAs, retrieval latency, generation latency, and workflow backlog health. | Pilot MVP |
| Context cache and memory tests | Verify cached/memory context respects access, freshness, fact state, source revision, and invalidation rules. | Scale |
| Permission mirror tests | Verify source ACLs, federated connector scopes, read-only connector posture, and generated packet eligibility. | Scale |
| Release and migration management | Version ontology schema, rule changes, mapping changes, store migrations, and generated view templates. | Scale |
| Export and interoperability jobs | Export selected ontology slices, briefs, evidence packets, and audit history to approved enterprise destinations. | Scale |

## Feature Inventory

### 1. Ontology Authoring And Semantic Governance

| Feature | Description | Sequence |
|---------|-------------|----------|
| Core ontology workspace | Steward-facing place to maintain classes, relationships, properties, controlled vocabularies, and definitions. | Foundation |
| Concept and relationship review workflow | Lightweight approval flow for semantic changes, relationship additions, and vocabulary updates. | Foundation |
| Versioned ontology releases | Publish reviewed ontology versions with change notes and compatibility guidance. | Foundation |
| Federated extension registry | Track domain extensions, enterprise overlays, and source-specific mappings without polluting the core. | Scale |
| Research anchor registry | Keep progressive-disclosure evidence for modeling decisions separate from the user-facing ontology vocabulary. | Foundation |

### 2. Source Profiles And Progressive Ingestion

| Feature | Description | Sequence |
|---------|-------------|----------|
| Source profile registry | Maintain source purpose, owner, authority scope, context role, capture mode, update mode, retrieval mode, freshness, and security policy. | Foundation |
| Mapping template and sample review | Define source-to-ontology mappings using sample records before automation. | Foundation |
| Source assertion capture | Store imported, seeded, inferred, and human-provided facts with provenance and fact state. | Pilot MVP |
| Authority and conflict handling | Detect conflicting source facts and apply declared authority rules. | Pilot MVP |
| Progressive ingestion lanes | Support landscape, integration, delivery, runtime, governance, and knowledge-source lanes at different maturity levels. | Scale |
| Freshness monitoring | Warn when facts are stale for the target gate or consumer view. | Pilot MVP |
| Permission and connector-mode monitoring | Detect when indexed, federated, read-only, or action-capable connectors drift from their declared source profile. | Scale |
| Source-correctness scoring | Track which sources and mappings produce accepted/rejected context so learning improves proposals without silently changing authority. | Scale |

### 3. Identity, Resolution, And Provenance

| Feature | Description | Sequence |
|---------|-------------|----------|
| Canonical identity model | Define stable identifiers for Product, DigitalExperience, SoftwareSystem, DeployableUnit, InterfaceContract, DataAsset, Control, and Evidence. | Foundation |
| Conservative entity matching | Propose matches across source tools using keys and evidence, but require review for risky merges. | Pilot MVP |
| Provenance explorer | Let reviewers see source system, source record, timestamp, mapping rule, fact state, and confidence. | Pilot MVP |
| Relationship evidence model | Show why a relationship exists and whether it is asserted, inferred, observed, or reviewed. | Pilot MVP |
| Learned relationship proposal model | Separate learned/proposed relationships from reviewed and authoritative relationships, including evidence, source usage, and rejection memory. | Scale |
| Merge/split correction workflow | Allow stewards to correct identity mistakes without losing history. | Scale |

### 4. Context Selection And Retrieval

| Feature | Description | Sequence |
|---------|-------------|----------|
| Context policy registry | Maintain context operation policies by data type and source profile. | Foundation |
| Context selector | Given a task, gate, anchor, access rights, and retrieval budget, select the facts that should ground the answer or packet. | Pilot MVP |
| Scoped graph retrieval | Retrieve neighborhood context around a Product, DigitalExperience, SoftwareSystem, or InterfaceContract. | Pilot MVP |
| Rule-triggered retrieval | Pull standards, NFRs, controls, risks, exceptions, and evidence when applicability rules fire. | Pilot MVP |
| Hybrid knowledge retrieval | Retrieve supporting documents, patterns, and research anchors without treating them as authoritative facts. | Scale |
| Live source query and summarization | Query high-velocity sources only when needed, and prefer summaries for telemetry/delivery state. | Scale |
| Synced/federated retrieval mode | Choose indexed retrieval for stable/shared context and live federated retrieval for volatile, sensitive, or high-volume context. | Scale |
| Context packet trace | Show included facts, excluded facts, fact states, freshness warnings, provenance, and elicitation requests. | Pilot MVP |
| Compiled context artifact | Persist task-specific context packets with source revisions, retrieval profile, included/excluded context, and generation eligibility. | Scale |
| Verified examples library | Maintain reviewed examples, expected answers, query patterns, and semantic-view examples that calibrate retrieval/generation. | Scale |

### 5. Agent/Tool/Action Governance And Context Learning

| Feature | Description | Sequence |
|---------|-------------|----------|
| Agent/tool registry | Maintain approved agents, tools, MCP resources, actions, owners, contracts, scopes, lifecycle state, and sensitive-action flags. | Scale |
| Tool/action selection policy | Decide which tools or actions are relevant and allowed for a task, gate, user, anchor, and source context. | Scale |
| Action provenance model | Record why a tool/action was selected, what context grounded it, what policy allowed it, and what outcome occurred. | Scale |
| Trace capture and summarization | Capture redacted retrieval traces, tool-selection outcomes, reviewer corrections, and failed-context events. | Scale |
| Learned graph proposal workflow | Propose candidate entities, relationships, source mappings, and wiki/manifest updates from traces, documents, and contracts. | Scale |
| Proposal confidence and evidence model | Store extraction method, supporting sources, confidence, conflicts, reviewer disposition, and promotion history. | Scale |
| Context-learning metrics | Measure repeated missing context, over-selected context, stale sources, rejected proposals, accepted proposals, and reviewer trust. | Scale |

### 6. Gate Validation And Applicability

| Feature | Description | Sequence |
|---------|-------------|----------|
| Gate profile registry | Configure required attributes, severity, evidence expectations, and allowed fact states by gate. | Foundation |
| Validation rule library | Define identity, ownership, integration, data, SDLC, runtime, risk, control, and source-authority checks. | Pilot MVP |
| Applicability rule authoring | Govern rules that decide which standards, NFRs, controls, or regulations apply to a target. | Pilot MVP |
| Exception and risk acceptance workflow | Capture exceptions with owner, rationale, approver, expiry, and review date. | Pilot MVP |
| Evidence requirement tracking | Distinguish planned evidence from actual evidence by gate. | Pilot MVP |
| Deployment gate hardening | Add stricter production-readiness evidence, operational controls, and attestation rules. | Scale |

### 7. Consumer Views And Solution Design Experience

| Feature | Description | Sequence |
|---------|-------------|----------|
| Solution Design Brief generator | Generate the first accepted consumer view from governed ontology facts and context traces. | Pilot MVP |
| Application landscape view | Show product/system composition, owners, lifecycle, criticality, and dependencies. | Pilot MVP |
| Integration surface view | Show APIs, events, files, queues, tool contracts, providers, consumers, and data exchanged. | Pilot MVP |
| Required attribute view | Show solution-design attributes by target gate, including missing and candidate facts. | Pilot MVP |
| Standards, NFR, and control view | Show applicable constraints, requirements, evidence, risks, exceptions, and decision posture. | Pilot MVP |
| Progressive disclosure drill-down | Move from summary to design facts, evidence, source records, rules, and research anchors. | Pilot MVP |
| Reviewer decision capture | Capture gate decisions, unresolved questions, owners, and planned evidence. | Scale |

### 8. Human Elicitation And Review Workflow

| Feature | Description | Sequence |
|---------|-------------|----------|
| Elicitation prompt queue | Turn missing or ambiguous context into structured questions for the right owner or reviewer. | Pilot MVP |
| Candidate fact capture | Store elicited answers as candidate facts unless the reviewer is authorized to approve them. | Pilot MVP |
| Review assignment and reminders | Route candidate facts, conflicts, and exceptions to stewards or accountable owners. | Scale |
| Human correction capture | Allow reviewers to correct AI-selected context and feed corrections into curation. | Scale |
| Review outcome promotion | Promote candidate facts to reviewed or authoritative states only through approved workflow. | Pilot MVP |

### 9. Security, Access, And Trust

| Feature | Description | Sequence |
|---------|-------------|----------|
| Access policy model | Define who can see, author, review, approve, or retrieve each class of context. | Foundation |
| Classification-aware retrieval | Enforce data classification and redaction before facts appear in AI context or user views. | Pilot MVP |
| Audit trail | Record source changes, ontology changes, context packets, gate decisions, and fact promotions. | Pilot MVP |
| Trust boundary indicators | Show whether facts are authoritative, reviewed, candidate, inferred, stale, or disputed. | Pilot MVP |
| Sensitive-context suppression | Prevent secrets, credentials, restricted source records, and unsupported inferred facts from being retrieved by default. | Foundation |

### 10. Pilot Operations And Measurement

| Feature | Description | Sequence |
|---------|-------------|----------|
| Pilot workspace | Define the pilot anchor, scope, source lanes, reviewers, gates, and success criteria. | Pilot MVP |
| Seeded pilot instance | Maintain the first worked product/system slice before full automation. | Pilot MVP |
| Usefulness metrics | Measure time saved, missing-attribute discovery, reviewer confidence, source conflict rate, and decision quality. | Pilot MVP |
| Expansion readiness review | Decide whether to add sources, gates, domains, or federated ontology extensions. | Scale |
| Operating dashboard | Track freshness, validation posture, ingestion health, review backlog, and adoption. | Scale |

## Sequencing

### Foundation

Goal: make the ontology governable before trying to scale ingestion or AI use.

Build or enable:

- Core ontology workspace.
- Concept and relationship review workflow.
- Versioned ontology releases.
- Research anchor registry.
- Source profile registry.
- Mapping template and sample review.
- Context attachment schema and document/evidence metadata profile.
- Canonical identity model.
- Context policy registry.
- Gate profile registry.
- Initial agent/tool/action governance model, even if the first implementation is file-based.
- Access policy model.
- Sensitive-context suppression.
- Graph store for canonical entities and relationships.
- Relational database for configuration, workflow state, and source profiles.
- Object store for source snapshots, evidence files, and generated packet archives.
- Rules store for gate profiles, validation rules, and applicability rules.
- IAM integration, secrets/key management, and baseline access control.
- Infrastructure as code and CI/CD for ontology services.
- Synthetic pilot dataset and mapping test harness.

Exit criteria:

- The pilot concepts, relationships, source profiles, context policies, gate profiles, and access assumptions are reviewable.
- A reviewer can tell what is core semantic meaning, what is source fact, what is evidence, and what is candidate context.
- The minimum platform can store graph facts, workflow/configuration records, source/evidence artifacts, and governed rules.
- Tool/action context is modeled as governed context, not as implicit capability hidden behind the AI runtime.

### Pilot MVP

Goal: prove that a small governed slice can generate a useful Solution Design Brief.

Build or enable:

- Source assertion capture.
- Authority and conflict handling.
- Freshness monitoring.
- Batch ingestion pipeline.
- Repository scanner.
- Contract scanner.
- Document ingestion, chunking, and metadata enrichment.
- Embedding pipeline.
- Data catalog sync.
- GRC/control sync.
- CI/CD evidence collector.
- Source schema drift detector.
- Quarantine and dead-letter pipeline.
- Fact promotion pipeline.
- Conservative entity matching.
- Provenance explorer.
- Relationship evidence model.
- Initial compiled context packet artifact for the pilot brief.
- Vector store or vector index.
- Search index.
- Queue or event bus.
- Compute runtime for ingestion and validation jobs.
- API gateway.
- Context selector.
- Scoped graph retrieval.
- Rule-triggered retrieval.
- Context packet trace.
- Validation rule library.
- Applicability rule authoring.
- Exception and risk acceptance workflow.
- Evidence requirement tracking.
- Solution Design Brief generator.
- Application landscape, integration, required-attribute, and standards/control views.
- Progressive disclosure drill-down.
- Elicitation prompt queue.
- Candidate fact capture.
- Review outcome promotion.
- Basic trace/correction capture for pilot feedback, even if full trace learning remains later.
- Classification-aware retrieval.
- Audit trail.
- Trust boundary indicators.
- Ontology steward console.
- Source profile and mapping workbench.
- Ingestion run monitor.
- Review queue and approval inbox.
- Context trace and provenance viewer.
- Pilot workspace.
- Seeded pilot instance.
- Usefulness metrics.

Exit criteria:

- A Solution Design Brief can be generated from the pilot slice.
- The brief shows included context, excluded context, fact states, provenance, missing required attributes, and human questions.
- The pilot can show whether missing or wrong context should become an elicitation question, wiki update, manifest update, or candidate graph proposal.
- Reviewers can decide whether the ontology helped the solution-design approval conversation.
- The pilot can ingest or seed facts, store them with provenance, retrieve context across graph/search/vector paths, validate gate posture, and expose reviewable UI workflows.

### Scale

Goal: move from a useful pilot to repeatable enterprise adoption.

Build or enable:

- Federated extension registry.
- Progressive ingestion lanes.
- Merge/split correction workflow.
- Hybrid knowledge retrieval.
- Live source query and summarization.
- Synced/federated connector modes.
- Object/storage annotation sync.
- Catalog semantic enrichment sync.
- Context compilation service and context compiler.
- Verified example ingestion and verified examples library.
- Grounding and answer evaluation service.
- Semantic cache and semantic cache policy.
- Agent memory store.
- Source-correctness scoring service and source-quality learning loop.
- Agent/tool registry API, registry console, and tool/resource selector service.
- Agent/tool registry connector.
- Trace/event store.
- Trace learning service.
- Graph proposal workspace and learned graph proposal queue.
- Agent trace ingestion and summarization.
- Golden-trace regression tests.
- Golden-answer evaluation tests.
- Permission mirror tests.
- Context learning loop.
- Event/webhook ingestion pipeline.
- Observability summarizer.
- Cache for ontology metadata, context packets, and source freshness summaries.
- Backup, restore, and disaster recovery.
- Retention and lifecycle management.
- AI response evaluator.
- Model/provider adapter.
- Deployment gate hardening.
- Reviewer decision capture.
- Review assignment and reminders.
- Human correction capture.
- Expansion readiness review.
- Operating dashboard.

Exit criteria:

- Additional domains, tools, and gates can onboard without changing the core ontology meaning.
- The system can distinguish stable semantic context from living operational context.
- Governance, validation, and context-selection behavior are measurable.

## Capability Dependency Order

1. Ontology authoring and semantic governance.
2. Access policy and trust boundaries.
3. Source profiles and authority matrix.
4. Context attachment schema, catalog semantic enrichment, and source-object/document metadata.
5. Identity, source assertions, provenance, and conservative resolution.
6. Context policy registry, context selector, and compiled context artifacts.
7. Agent/tool/action registry and action selection policy.
8. Gate profiles, validation rules, and applicability rules.
9. Consumer views and Solution Design Brief generation.
10. Human elicitation, trace feedback, and review workflows.
11. Pilot measurement and expansion readiness.
12. Federated extensions, live retrieval, learned graph proposals, semantic cache/memory, and enterprise-scale operations.

## Review Questions

- Which capability group should be treated as the first delivery epic?
- Which features are required for the first live pilot versus acceptable as manual process?
- Which features must exist before any AI-generated Solution Design Brief is trusted by reviewers?
- Which source systems should be enabled first for the pilot MVP?
- Which gate should be the first validation target: Solution Design Approval, Build Readiness, or Deploy to Production?
- Which features require enterprise security, GRC, or platform approval before the pilot can run?
- Should this roadmap become an implementation backlog, an architecture epic map, or both?
