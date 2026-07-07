---
type: ontology-roadmap
title: IT SDLC Ontology Reduced Feature Roadmap
slug: it-sdlc-ontology-reduced-feature-roadmap
project: unified-knowledge-base
status: drafted
provenance: ai-assisted
created: 2026-06-30
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - it-sdlc-ontology
  - reduced-feature-roadmap
  - controlled-wiki
  - context-selection
---

## Purpose

This artifact translates the reduced-build decision into a concrete feature roadmap.

The full feature roadmap describes the eventual platform-scale implementation. This reduced roadmap describes the smallest credible pilot implementation using Git, repo-local manifests, controlled AI-written wiki pages, rebuildable indexes, and a thin context-selection layer.

It should answer:

> What do we actually need to build or enable for the reduced pilot, and what can remain manual, existing-tool-based, or deferred?

## Placement Decision

This roadmap is not a numbered phase.

It sits beside:

- [[it-sdlc-ontology-feature-roadmap]] for the full platform roadmap.
- [[it-sdlc-ontology-build-reduction-options]] for the decision rationale.

Use this roadmap as the pilot delivery backlog unless review determines that the reduced build cannot answer the Phase 1 competency questions or produce the Phase 9 Solution Design Brief View.

## Reduced Capability Map

| Capability Group | What It Enables | Reduced Build Stance |
|------------------|-----------------|----------------------|
| Git-backed ontology and rules | Maintain core semantics, context policy, gate profiles, and validation rules as reviewed files. | Build with Markdown/YAML and PR review. |
| Repo-local context manifests | Capture software/system/interface facts where the work lives. | Build lightweight schema, examples, and linting. |
| Value-stream rollup | Aggregate cross-repo context without centralizing all source code or tool data. | Build generated rollup in a value-stream meta-repo. |
| Controlled AI-written wiki | Maintain living narrative context with AI drafts and human approval. | Use existing wiki or Markdown docs with approval workflow. |
| Context attachment metadata | Attach business context, freshness, source links, and usage rules near documents, wiki pages, evidence snapshots, and source-object links. | Use frontmatter and small metadata sidecars, not object-store automation. |
| Rebuildable context indexes | Search, vector, and derived graph indexes over Git, wiki, ADRs, and selected contracts. | Build as derived indexes, not authoritative stores. |
| File-based agent/tool/action registry | Describe approved tools, actions, MCP resources, contracts, scopes, and constraints for context-aware agent use. | Build as reviewed manifests, not a custom registry service. |
| Thin context selector | Select context by anchor, gate, fact state, freshness, source, and access. | Build focused selector for Solution Design Brief generation. |
| Brief generation and traceability | Generate Solution Design Briefs with provenance, context trace, and missing questions. | Build first consumer workflow only. |
| Task-specific context compilation | Materialize the selected context packet before generation, so the brief is grounded in a reviewable artifact. | Treat the Solution Design Brief packet as the first compiled context artifact. |
| Elicitation and writeback | Convert missing context into questions and approved updates. | Write back to wiki or Git-reviewed manifests. |
| Trace-driven context improvement | Use reviewer corrections, failed retrievals, and agent traces to propose context updates. | Generate candidate wiki/manifest/graph proposals for review. |
| Minimal validation | Check required attributes and fact states for Solution Design Approval. | Use YAML gate profiles plus deterministic checks. |
| Pilot measurement | Measure usefulness, trust, context quality, and build reduction effectiveness. | Lightweight metrics and review notes. |

## Reduced Implementation Build Surfaces

The reduced pilot should still be explicit about build surfaces. The difference is that many surfaces use Git, wiki, generated indexes, or existing systems instead of custom platform services.

### User Interfaces And Human Workflows

| Feature | What To Build Or Use | Sequence |
|---------|----------------------|----------|
| Git review workflow | Use pull requests or equivalent review for ontology files, source profiles, gate profiles, and repo manifests. | Reduced Foundation |
| Wiki review workflow | Use controlled wiki review for AI-written pages and updates. | Reduced Foundation |
| Context attachment review | Review frontmatter/metadata sidecars for wiki pages, ADRs, design docs, evidence snapshots, and source-object links. | Reduced Foundation |
| Solution Design Brief review page | Render generated brief, context trace, findings, and elicitation questions in Markdown/wiki form. | Reduced Pilot MVP |
| Context trace drill-down page | Render included facts, excluded facts, source links, fact states, freshness, and citations. | Reduced Pilot MVP |
| Retrieved snippet review | Render the exact snippets, rows, links, manifest fields, and graph edges selected for the compiled context packet. | Reduced Pilot MVP |
| Elicitation question list | Maintain structured open questions and owners in wiki, issue tracker, or Markdown. | Reduced Pilot MVP |
| Tool/action review list | Maintain approved tools, actions, contracts, owners, scopes, and constraints as reviewed files or pages. | Reduced Foundation |
| Proposed context update queue | Review candidate wiki edits, manifest changes, and graph edges proposed from traces or reviewer corrections. | Reduced Pilot MVP |
| Pilot review board | Lightweight review ritual using generated brief plus notes, not a custom UI. | Reduced Pilot MVP |
| Index health page | Simple generated page showing indexed repos, wiki pages, stale pages, broken links, and last refresh. | Reduced Pilot MVP |

### APIs, Scripts, And Thin Services

| Feature | What To Build Or Use | Sequence |
|---------|----------------------|----------|
| Manifest schema and linter | JSON Schema or equivalent validation for repo-local `ontology-context.yaml` files. | Reduced Foundation |
| Ontology file validator | Script to validate core ontology files, relationship definitions, context policies, and gate profiles. | Reduced Foundation |
| Source profile validator | Script to validate source profile files and required context policy fields. | Reduced Foundation |
| Context attachment validator | Script to validate page/document/evidence metadata such as owner, source links, access classification, freshness, usage rules, and fact state. | Reduced Foundation |
| Context indexer | Script/service that reads Git repos, wiki pages, ADRs, contracts, and selected metadata into rebuildable indexes. | Reduced Pilot MVP |
| Derived graph builder | Script that turns manifests, wiki frontmatter, relationships, and contracts into a lightweight graph representation. | Reduced Pilot MVP |
| Context selector | Thin service or script that selects context for the Solution Design Brief based on anchor and gate. | Reduced Pilot MVP |
| Tool/resource selector | Thin rules that select allowed tools, actions, and resources relevant to the design context without invoking them by default. | Reduced Pilot MVP |
| Context packet compiler | Materializes task-specific selected context, provenance, excluded context, freshness warnings, and retrieval decisions before generation. | Reduced Pilot MVP |
| Context packet evaluator | Lightweight checks for unsupported claims, missing citations, wrong snippets, hidden candidate facts, and stale/restricted context. | Reduced Pilot MVP |
| Validation runner | Deterministic rule runner for required attributes, allowed fact states, missing evidence, and blocking/advisory findings. | Reduced Pilot MVP |
| Brief generator | Template-driven generation of the Solution Design Brief from selected context and validation findings. | Reduced Pilot MVP |
| Elicitation writer | Script/service that converts missing context into structured questions and draft wiki updates. | Reduced Pilot MVP |
| Trace summarizer | Script/service that turns approved traces, reviewer corrections, and failed retrievals into candidate update proposals. | Reduced Pilot MVP |
| Graph proposal builder | Script that proposes candidate relationships from manifests, contracts, traces, and wiki links without promoting them automatically. | Reduced Pilot MVP |
| Link checker | Script that checks source links, contract links, evidence links, and wiki cross-references. | Reduced Pilot MVP |
| Export packet generator | Produces reviewable Markdown/PDF-ready packet from brief, trace, findings, and decisions. | Later Reduced Scale |

### Git, Wiki, And Source Inputs

| Feature | What To Build Or Use | Sequence |
|---------|----------------------|----------|
| Core ontology repo/folder | Store core classes, relationships, context policy, gate profiles, and validation rules. | Reduced Foundation |
| Repo-local manifests | Add lightweight `ontology-context.yaml` or use existing `catalog-info.yaml` where available. | Reduced Foundation |
| Value-stream meta-repo | Aggregate cross-component context and generated rollups. | Reduced Pilot MVP |
| Controlled wiki namespace | Dedicated wiki space where AI can propose pages and humans approve them. | Reduced Foundation |
| Document and evidence frontmatter | Add metadata to wiki pages, ADRs, design docs, runbooks, evidence snapshots, and source-object links. | Reduced Foundation |
| Verified examples and query notes | Store reviewed examples, query patterns, expected answers, and semantic-view notes as non-authoritative context attachments. | Reduced Foundation |
| ADR and design-doc ingestion | Read existing Markdown design docs and ADRs as cited context, not authoritative facts. | Reduced Pilot MVP |
| Contract ingestion | Read OpenAPI, AsyncAPI, schemas, and tool contracts from repos. | Reduced Pilot MVP |
| Agent/tool/action manifests | Read reviewed manifests for approved agents, tools, MCP resources, actions, invocation constraints, and owners. | Reduced Foundation |
| Source links | Store links to CMDB, GRC, data catalog, CI/CD, observability, and work tracking instead of replicating them. | Reduced Pilot MVP |
| Trace and correction logs | Read redacted review corrections and approved agent trace summaries as inputs for proposed context updates. | Reduced Pilot MVP |
| Optional source snapshots | Capture small evidence snapshots only when source links are insufficient for review. | Later Reduced Scale |

### Data Stores, Indexes, And Infrastructure

| Feature | What To Build Or Use | Sequence |
|---------|----------------------|----------|
| Git repository storage | Authoritative storage for stable ontology specs, manifests, and value-stream rollups. | Reduced Foundation |
| Controlled wiki storage | Authoritative storage for approved living narrative context. | Reduced Foundation |
| File/object storage | Store generated packets, optional source snapshots, and evidence attachments only if needed. | Reduced Pilot MVP |
| Compiled context packet archive | Store generated Solution Design Brief context packets for review, regression, and audit when needed. | Reduced Pilot MVP |
| Search index | Keyword/faceted search over ontology files, manifests, wiki pages, ADRs, contracts, and packet artifacts. | Reduced Pilot MVP |
| Vector index | Semantic retrieval over wiki pages, ADRs, standards, design docs, and selected evidence. | Reduced Pilot MVP |
| Derived graph file/index | Rebuildable graph from manifests, wiki frontmatter, contracts, and ontology relationships. | Reduced Pilot MVP |
| Candidate graph proposal file/index | Reviewable proposed edges and entities from traces, contracts, documents, and extraction jobs. | Reduced Pilot MVP |
| Lightweight metadata store | Small relational or document store only if files are insufficient for run state, index status, and review metadata. | Later Reduced Scale |
| Scheduled job runner | Runs indexing, linting, graph build, link checks, and packet generation. | Reduced Pilot MVP |
| Access controls | Reuse Git, wiki, and source-system access controls; enforce scopes before indexing/retrieval. | Reduced Foundation |
| Permission mirror checks | Verify indexed or compiled context respects source ACLs and read-only connector posture. | Reduced Foundation |
| Audit trail | Reuse Git history, wiki revision history, and generated packet logs. | Reduced Foundation |

### AI, Retrieval, And Context Runtime

| Feature | What To Build Or Use | Sequence |
|---------|----------------------|----------|
| Retrieval profile for Solution Design Approval | Defines context sources, filters, allowed fact states, freshness checks, and evidence requirements. | Reduced Foundation |
| Context selector prompt/rules | Deterministic rules plus prompt instructions for selecting relevant context from indexes. | Reduced Pilot MVP |
| Tool/action context rules | Select allowed tool/action context by anchor, gate, user, source, contract, and approval status. | Reduced Pilot MVP |
| Context packet assembler | Produces included facts, excluded facts, source links, freshness warnings, and elicitation questions. | Reduced Pilot MVP |
| Context compilation manifest | Records the compiled packet's anchor, gate, retrieval profile, source revisions, included facts, excluded facts, and freshness posture. | Reduced Pilot MVP |
| Brief template | Versioned template for the Solution Design Brief and context trace. | Reduced Pilot MVP |
| Citation guardrail | Requires generated claims to cite selected facts, wiki pages, repo manifests, contracts, or source links. | Reduced Pilot MVP |
| Snippet grounding guardrail | Requires generated claims to map to selected snippets, rows, fields, graph edges, or manifest entries in the compiled packet. | Reduced Pilot MVP |
| Candidate fact guardrail | Keeps unapproved AI-written or elicited facts visibly marked as candidate. | Reduced Pilot MVP |
| Trace-to-update guardrail | Keeps trace-derived proposals separate from reviewed facts until approved. | Reduced Pilot MVP |
| Human correction capture | Captures reviewer corrections as wiki update drafts or manifest PRs. | Later Reduced Scale |

### DevOps, Testing, And Operations

| Feature | What To Build Or Use | Sequence |
|---------|----------------------|----------|
| Repo templates | Starter folder/files for ontology specs, source profiles, repo manifests, and value-stream rollups. | Reduced Foundation |
| CI checks for manifests | Lint manifests, context policy fields, source links, and required identifiers. | Reduced Foundation |
| CI checks for context attachments | Validate wiki/document/evidence frontmatter and source-object metadata sidecars. | Reduced Foundation |
| CI checks for tool/action manifests | Validate owners, scopes, contracts, approval status, and sensitive action flags. | Reduced Foundation |
| Permission mirror test | Verify source ACLs, restricted links, and read-only connector assumptions are preserved before indexing or packet compilation. | Reduced Foundation |
| CI checks for ontology files | Validate class/relationship references, gate profiles, and validation rule syntax. | Reduced Foundation |
| Golden brief regression test | Fixed pilot input produces expected brief structure, trace fields, and validation findings. | Reduced Pilot MVP |
| Golden context packet regression test | Fixed pilot input produces expected compiled context packet contents, exclusions, and freshness warnings. | Reduced Pilot MVP |
| Golden answer evaluation test | Fixed pilot packet catches missing citations, unsupported claims, wrong snippets, and hidden candidate facts. | Reduced Pilot MVP |
| Golden trace regression test | Fixed reviewer corrections or trace summaries produce expected candidate update proposals. | Reduced Pilot MVP |
| Index rebuild job | Rebuild search/vector/graph indexes from Git/wiki sources on schedule or PR merge. | Reduced Pilot MVP |
| Broken-link and freshness check | Detect stale wiki pages, dead source links, missing owners, and outdated manifests. | Reduced Pilot MVP |
| Access leakage test | Verify restricted pages and source links do not enter unauthorized context packets. | Reduced Pilot MVP |
| Pilot metrics report | Generated report on indexed sources, missing attributes, questions asked, reviewer changes, and decision outcome. | Reduced Pilot MVP |

## Reduced Feature Inventory

### 1. Git-Backed Ontology And Rules

| Feature | Description | Sequence |
|---------|-------------|----------|
| Core ontology files | Markdown/YAML files for concepts, relationships, context operation policies, and controlled vocabularies. | Reduced Foundation |
| Gate profile files | YAML/Markdown profiles for Solution Design Approval required attributes and allowed fact states. | Reduced Foundation |
| Validation rule files | Deterministic rules for missing ownership, missing interface context, candidate data classification, and candidate control applicability. | Reduced Foundation |
| Source profile files | Source authority, freshness, retrieval mode, and elicitation policy captured as files. | Reduced Foundation |
| Context attachment schema | Frontmatter/sidecar schema for documents, wiki pages, evidence snapshots, and source-object links. | Reduced Foundation |
| PR review checklist | Checklist for semantic changes, source mappings, context policy changes, and validation changes. | Reduced Foundation |

### 2. Repo-Local Context Manifests

| Feature | Description | Sequence |
|---------|-------------|----------|
| Manifest schema | Minimal schema for product/system/deployable/interface ownership, contracts, data, and source links. | Reduced Foundation |
| Manifest examples | Example files for micro-frontend, BFF, backend service, event publisher, and agentic app. | Reduced Foundation |
| Manifest linter | Validates required fields, stable IDs, owners, links, and declared relationships. | Reduced Foundation |
| Component repo rollout | Add manifests to the pilot component repos or a representative seed folder. | Reduced Pilot MVP |
| Value-stream manifest rollup | Generate a cross-repo rollup of systems, units, contracts, data, and ownership. | Reduced Pilot MVP |

### 3. Controlled AI-Written Wiki

| Feature | Description | Sequence |
|---------|-------------|----------|
| Wiki namespace and page types | Dedicated wiki area for product, system, interface, gate decision, exception, and pattern pages. | Reduced Foundation |
| Wiki frontmatter schema | Owner, scope, source links, access classification, fact state, freshness, and review status. | Reduced Foundation |
| Evidence/source-object metadata | Metadata for evidence snapshots and source-object links, including source owner, source record, access scope, freshness, and usage rules. | Reduced Foundation |
| Verified example pages | Reviewed examples, query patterns, expected answers, and semantic interpretations that help calibrate context selection without becoming authoritative facts. | Reduced Foundation |
| AI draft update flow | AI proposes page creation/update from review notes, repo facts, and elicitation answers. | Reduced Pilot MVP |
| Human approval flow | Humans approve, edit, reject, or route proposed wiki updates. | Reduced Pilot MVP |
| Candidate fact marking | AI-written and elicited facts remain candidate until reviewed. | Reduced Pilot MVP |
| Wiki freshness check | Detect stale pages and route review questions to owners. | Reduced Pilot MVP |

### 4. Rebuildable Context Indexes

| Feature | Description | Sequence |
|---------|-------------|----------|
| Source crawler | Reads ontology files, repo manifests, wiki pages, ADRs, contracts, and selected source links. | Reduced Pilot MVP |
| Search index | Keyword/faceted index for IDs, owners, source links, gates, contracts, controls, and pages. | Reduced Pilot MVP |
| Vector index | Semantic index for narrative wiki pages, ADRs, standards, design docs, and patterns. | Reduced Pilot MVP |
| Derived graph index | Lightweight graph of products, systems, deployable units, contracts, data, controls, evidence, and wiki pages. | Reduced Pilot MVP |
| Index provenance | Each indexed chunk/fact records file/page/source, revision, owner, access classification, and freshness. | Reduced Pilot MVP |
| Index rebuild report | Generated status page showing what was indexed and what failed. | Reduced Pilot MVP |

### 5. Agent/Tool Registry And Trace-Driven Improvement

| Feature | Description | Sequence |
|---------|-------------|----------|
| Tool/action manifest schema | Minimal schema for approved tools, MCP resources, actions, owners, contracts, permissions, invocation constraints, and sensitive-action flags. | Reduced Foundation |
| Tool/action manifest linter | Validates owners, scopes, contracts, approval status, source links, and restricted-action markers. | Reduced Foundation |
| Tool/action context selection | Adds relevant approved tool/action context to the Solution Design Brief without automatically invoking tools. | Reduced Pilot MVP |
| Trace and correction capture | Captures reviewer corrections, failed retrievals, and approved trace summaries as input to candidate updates. | Reduced Pilot MVP |
| Candidate graph proposal generation | Proposes new or changed relationships from contracts, traces, and reviewer corrections, with fact state set to candidate. | Reduced Pilot MVP |
| Candidate update review | Routes proposed wiki edits, manifest changes, and graph edges through human review before promotion. | Reduced Pilot MVP |

### 6. Thin Context Selector And Brief Generator

| Feature | Description | Sequence |
|---------|-------------|----------|
| Retrieval profile | Defines what context is needed for Solution Design Approval. | Reduced Foundation |
| Anchor resolver | Resolves Product, DigitalExperience, or SoftwareSystem anchor from manifest/wiki/source links. | Reduced Pilot MVP |
| Context selection rules | Select by gate, anchor, fact state, source authority, freshness, access, and retrieval budget. | Reduced Pilot MVP |
| Tool/action selection rules | Select approved action/tool context when it affects integration, automation, or agentic solution design. | Reduced Pilot MVP |
| Context packet builder | Produces compact selected context with included/excluded facts and citations. | Reduced Pilot MVP |
| Compiled context packet | Materialized packet that can be reviewed before generation and archived after gate review. | Reduced Pilot MVP |
| Snippet and field grounding map | Links generated brief claims back to selected snippets, fields, rows, graph edges, manifest entries, and verified examples. | Reduced Pilot MVP |
| Solution Design Brief generator | Generates the brief, validation findings, context trace, and decisions needed. | Reduced Pilot MVP |
| Citation and fact-state checks | Blocks or flags generated claims without citation or with hidden candidate facts. | Reduced Pilot MVP |

### 7. Minimal Validation And Elicitation

| Feature | Description | Sequence |
|---------|-------------|----------|
| Required attribute checks | Check ownership, lifecycle, system composition, interface contracts, data classification, NFRs, controls, risks, and evidence plan. | Reduced Pilot MVP |
| Fact-state checks | Block or warn when required facts are candidate, inferred, disputed, stale, or missing. | Reduced Pilot MVP |
| Elicitation prompt generation | Turn missing or ambiguous context into questions for owners/reviewers. | Reduced Pilot MVP |
| Elicitation answer capture | Store answers as candidate facts in wiki pages or manifest PRs. | Reduced Pilot MVP |
| Review outcome capture | Record accepted risks, deferred evidence, and reviewer decisions in wiki or generated packet. | Reduced Pilot MVP |

### 8. Pilot Operations And Measurement

| Feature | Description | Sequence |
|---------|-------------|----------|
| Pilot source inventory | List repos, wiki spaces, contracts, ADRs, and authoritative source links included in the pilot. | Reduced Foundation |
| Pilot seed pack | Seeded product/system/component/interface/data/control example for regression and demos. | Reduced Foundation |
| Pilot run script | One command or workflow to lint, index, validate, select context, and generate the brief. | Reduced Pilot MVP |
| Pilot metrics | Track missing facts, candidate facts, elicitation questions, reviewer changes, stale links, and brief usefulness. | Reduced Pilot MVP |
| Expansion decision checklist | Decide whether to continue reduced build, add adapters, or graduate to the full roadmap. | Later Reduced Scale |

## Reduced Sequencing

### Reduced Foundation

Goal: make the reduced pilot reviewable and governable without custom platform build.

Build or enable:

- Core ontology files.
- Gate profile files.
- Validation rule files.
- Source profile files.
- Context attachment schema.
- Manifest schema, examples, and linter.
- Tool/action manifest schema and linter.
- Wiki namespace and frontmatter schema.
- Verified example and query-note page pattern.
- Retrieval profile for Solution Design Approval.
- Repo templates.
- CI checks for ontology files, manifests, and tool/action manifests.
- CI checks for context attachments.
- Permission mirror test.
- Pilot source inventory.
- Pilot seed pack.
- Access controls using existing Git/wiki/source permissions.
- Audit trail through Git history and wiki revision history.

Exit criteria:

- A reviewer can inspect the ontology, context policy, source profiles, manifests, wiki page schema, and gate profile as files/pages.
- Pilot repos or seed folders can declare systems, deployable units, contracts, ownership, and source links.
- The controlled wiki has approved page types and review expectations.
- The approved tools, resources, actions, contracts, scopes, and sensitive-action constraints are visible as reviewed files/pages.

### Reduced Pilot MVP

Goal: generate a useful Solution Design Brief without building the full platform.

Build or enable:

- Component repo manifests for the pilot.
- Value-stream manifest rollup.
- AI draft update flow and human approval flow.
- Candidate fact marking.
- Source crawler.
- Search index.
- Vector index.
- Derived graph index.
- Candidate graph proposal file/index.
- Index provenance and rebuild report.
- Tool/resource selector.
- Context selector.
- Anchor resolver.
- Context packet builder.
- Context packet compiler and compilation manifest.
- Context packet evaluator.
- Solution Design Brief generator.
- Citation and fact-state checks.
- Required attribute checks.
- Elicitation prompt generation.
- Elicitation answer capture.
- Review outcome capture.
- Trace summarizer and graph proposal builder.
- Proposed context update queue.
- Solution Design Brief review page.
- Context trace drill-down page.
- Elicitation question list.
- Index health page.
- Golden brief regression test.
- Golden answer evaluation test.
- Index rebuild job.
- Broken-link and freshness check.
- Access leakage test.
- Pilot metrics report.

Exit criteria:

- A pilot brief can be generated from Git, wiki, manifests, contracts, and source links.
- The brief includes context trace, provenance, fact states, missing attributes, and elicitation questions.
- Tool/action context can be shown when relevant to integration or agentic solution design without granting implicit execution authority.
- Reviewers can approve, reject, or correct AI-written wiki updates and generated brief content.
- Reviewer corrections and trace summaries produce candidate updates rather than silent ontology changes.
- The team can tell whether a full graph store, workflow UI, or ingestion platform is justified.

### Later Reduced Scale

Goal: extend the reduced model only where the pilot proves value.

Build or enable:

- Optional source snapshots.
- Export packet generator.
- Lightweight metadata store for run state and review metadata, if file-based state becomes painful.
- Human correction capture.
- Expansion decision checklist.
- Additional repo manifests and value-stream rollups.
- Additional retrieval profiles for Build Readiness or Deploy to Production.
- Event-driven trace capture only if manual trace summaries repeatedly miss context-learning opportunities.
- First targeted source adapter only for a source that repeatedly blocks brief generation.

Exit criteria:

- More value streams can onboard with templates and generated checks.
- Repeated manual pain points are visible enough to justify targeted automation.
- Decisions to graduate to the full roadmap are evidence-based.

## Reduced Dependency Order

1. Core ontology, gate profile, source profile, and context policy files.
2. Context attachment schema for documents, wiki pages, evidence snapshots, and source-object links.
3. Repo-local manifest schema, examples, linter, and pilot manifests.
4. Tool/action manifest schema, examples, and linter.
5. Controlled wiki namespace, page schema, and approval workflow.
6. Source crawler, search index, vector index, derived graph index, and candidate graph proposal file.
7. Retrieval profile, context selector, tool/resource selector, context packet builder, and packet compiler.
8. Validation runner, elicitation prompt generation, and trace-to-update proposal generation.
9. Solution Design Brief generator and context trace pages.
10. Pilot metrics and expansion decision checklist.

## Explicit Deferrals

| Deferred Full-Roadmap Feature | Defer Until |
|-------------------------------|-------------|
| Custom ontology steward console | Git/wiki review becomes a bottleneck or nontechnical stewards cannot participate. |
| Full source profile UI | Source owners cannot maintain Markdown/YAML source profiles. |
| Production graph database | Derived graph cannot answer pilot traversal or impact questions. |
| Relational workflow app | Git/wiki/work-item workflow cannot handle review volume. |
| Broad connector framework | Repeated manual source-linking blocks generated briefs. |
| Custom tool registry service | File-based tool/action manifests cannot support review, discovery, permission modeling, or reuse. |
| Automated graph-learning service | Manual trace summaries and generated proposal files cannot keep up with reviewer corrections or discovered relationships. |
| Live event ingestion | Source freshness becomes a blocker for the target gate. |
| Observability summarizer | Runtime/operate questions become part of the pilot gate. |
| Full evidence repository | Existing links/snapshots cannot support review or audit. |
| Semantic cache or agent memory service | Repeated pilot tasks show material latency, cost, or continuity pain that compiled packets cannot address. |
| Managed answer-evaluation platform | Lightweight packet/claim checks cannot catch recurring unsupported answers. |
| Model/provider abstraction layer | More than one approved AI provider is required. |
| Operating dashboard | More than one value stream or pilot needs ongoing management. |

## Review Questions

- Is the reduced roadmap enough to generate the first trusted Solution Design Brief?
- Which pilot facts must live in repo manifests versus controlled wiki pages?
- Which existing wiki or documentation platform can support AI-write, human-review safely?
- Which repos or value-stream meta-repos should host the first manifests and rollups?
- Which source links are sufficient, and which source facts require snapshots or adapters?
- What is the minimum access-control model before indexing controlled wiki pages?
- What failure would force us to graduate from reduced build to the full feature roadmap?
