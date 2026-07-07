---
type: research-survey
title: IT SDLC Ontology Context Layer Vendor Patterns Survey
slug: it-sdlc-ontology-context-layer-vendor-patterns-survey
project: unified-knowledge-base
status: drafted
provenance: ai-assisted
created: 2026-06-30
modified: 2026-06-30
tags:
  - tiaa
  - unified-knowledge-base
  - it-sdlc-ontology
  - research-anchor
  - context-layer
  - living-ontology
---

> Discipline: applied (practitioner-pattern survey)

# IT SDLC Ontology Context Layer Vendor Patterns Survey

## Research Question

What should we learn from current context-layer, living-ontology, enterprise-search, agent-platform, developer-catalog, metadata-catalog, and lightweight implementation patterns, and does that change the reduced-build roadmap or the long-term feature roadmap?

The VentureBeat article supplied as the prompt anchor was not directly fetchable in this environment, but the user provided the article text on 2026-06-30. This survey now uses that supplied article text for the AWS Context/Horizon Context/Fabric IQ/Redis/Pinecone market framing and grounds recommendations in primary vendor documentation where available. Product claims that could not be verified from primary sources are marked with lower confidence.

## User-Supplied VentureBeat Article Signal

The article frames the context layer as a contested enterprise architecture category and highlights a new AWS stack composed of:

- A reported AWS Context service that automatically builds and improves a knowledge graph from existing enterprise data and agent usage.
- Amazon S3 Annotations as storage-layer business context attached to S3 objects.
- AWS Glue Data Catalog skill assets as catalog-layer domain knowledge, runbooks, query patterns, and usage rules.
- Steward review of inferred relationships before promotion to production.
- Runtime access for agents through agentic search APIs and MCP tools.
- Identity and data access inherited from IAM and Lake Formation.
- Metadata published in Apache Iceberg format to S3 Tables.

The article also names Snowflake Horizon Context/Cortex Sense, Microsoft Fabric IQ, Redis context platform, and Pinecone Nexus as adjacent context-layer offerings. Primary public documentation was found for Snowflake Horizon Catalog/Cortex Agents, Redis Iris, Pinecone Assistant, AWS S3 Tables, AWS Glue Data Catalog, AWS Lake Formation, and AWS AgentCore. Primary public documentation was not found during this pass for AWS Context, Amazon S3 Annotations, AWS Glue skill assets, Snowflake Horizon Context/Cortex Sense, Microsoft Fabric IQ, or Pinecone Nexus under those exact product names.

## Executive Synthesis

The research strengthens the reduced-build recommendation.

The leading pattern is not "build one central enterprise ontology app first." The leading pattern is a layered context system:

1. Keep source data in the systems that already own it.
2. Maintain a compact semantic core and source authority rules.
3. Build or reuse connectors, catalogs, and repo-local descriptors.
4. Materialize search, vector, and derived graph indexes for retrieval.
5. Add agent/tool/resource registries so context can be selected with the actions available.
6. Use traces, human corrections, and work conversations to propose context updates.
7. Promote learned context only through human review, source authority, and gate rules.

The long-term roadmap should expand to include agent/tool registry, trace-driven context improvement, synced versus federated connector patterns, and graph proposal governance. The reduced roadmap should add those ideas in file-based form, not as heavy platform build.

## Findings

- Finding: The user-supplied VentureBeat article makes AWS's differentiating claim that the graph should learn from agent usage and source correctness over time, while stewards review inferred relationships before production promotion. This is directly relevant to our model: learned graph edges should be proposals, not authoritative ontology updates. [low]
  Sources: User-supplied VentureBeat article text from 2026-06-30; [AWS S3 Tables](https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-tables.html), [AWS Glue Data Catalog](https://docs.aws.amazon.com/glue/latest/dg/components-overview.html), [AWS Lake Formation](https://docs.aws.amazon.com/lake-formation/latest/dg/what-is-lake-formation.html).
  Downgrade: single source for AWS Context; vendor-blogged adjacent sources; product-specific claims not independently verified.

- Finding: The strongest cross-vendor pattern is a layered context stack: storage/object annotations, catalog/semantic definitions, graph/index synthesis, runtime retrieval/tool APIs, and feedback loops from agent usage. [moderate]
  Sources: User-supplied VentureBeat article text from 2026-06-30; [Snowflake Horizon Catalog](https://docs.snowflake.com/en/user-guide/snowflake-horizon), [Redis Iris context engine](https://redis.io/docs/latest/develop/ai/context-engine/), [Pinecone Assistant](https://docs.pinecone.io/guides/assistant/overview), [AWS Glue Data Catalog](https://docs.aws.amazon.com/glue/latest/dg/components-overview.html), [MCP resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources).
  Downgrade: heterogeneity; vendor-blogged.

- Finding: AWS's reported context stack maps storage context, catalog context, graph context, and agent runtime context into separate layers. Public AWS docs verify S3 Tables/Iceberg, Glue Data Catalog, Lake Formation, and AgentCore-style agent infrastructure, but this pass did not find public primary docs for AWS Context, S3 Annotations, or Glue skill assets by exact name. [low]
  Sources: User-supplied VentureBeat article text from 2026-06-30; [AWS S3 Tables](https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-tables.html), [AWS Glue Data Catalog](https://docs.aws.amazon.com/glue/latest/dg/components-overview.html), [AWS Lake Formation](https://docs.aws.amazon.com/lake-formation/latest/dg/what-is-lake-formation.html), [AWS AgentCore Memory](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/memory.html), [AWS AgentCore Gateway](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway.html).
  Downgrade: single source for new AWS products; indirectness.

- Finding: Snowflake's public pattern is "agentic catalog plus governed semantic layer": Horizon Catalog positions itself as a context layer across data inside/outside Snowflake with semantic views, lineage, generated descriptions, object tagging, data quality, and query-engine governance; Cortex Agents add tool orchestration, MCP connectors, skills, monitoring, evaluations, and feedback. [moderate]
  Sources: [Snowflake Horizon Catalog](https://docs.snowflake.com/en/user-guide/snowflake-horizon), [Snowflake Cortex Agents](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-agents), [Snowflake Cortex Analyst](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-analyst), [Snowflake Cortex Search](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-search/cortex-search-overview).
  Downgrade: vendor-blogged.

- Finding: Redis's public pattern is a runtime context engine: semantic cache, persistent agent memory, governed tool generation from a data model, and live data sync from relational sources into Redis Cloud. For our roadmap, this supports adding semantic cache and agent memory as scale features, not pilot prerequisites. [moderate]
  Sources: [Redis Iris context engine](https://redis.io/docs/latest/develop/ai/context-engine/), [RedisVL](https://redis.io/docs/latest/develop/ai/redisvl/).
  Downgrade: vendor-blogged; single vendor.

- Finding: Pinecone's public pattern is managed retrieval and context compilation around proprietary documents: managed chunking/embedding/storage, grounded answers with citations, answer evaluation, context-snippet retrieval, metadata filtering, and MCP integration. The article's "Pinecone Nexus" claim could not be verified from primary public docs in this pass. [low]
  Sources: [Pinecone Assistant](https://docs.pinecone.io/guides/assistant/overview), [Pinecone database overview](https://docs.pinecone.io/guides/get-started/overview), user-supplied VentureBeat article text from 2026-06-30.
  Downgrade: single source for Nexus; vendor-blogged adjacent docs.

- Finding: Microsoft Fabric IQ and Microsoft IQ appear in current secondary reporting as a context layer spanning work signals, structured business data, and web grounding, with Fabric IQ positioned around semantic/ontological controls and MCP. Public primary documentation for Fabric IQ by exact name was not found in this pass, so it should be treated as a market signal rather than a directly modelable product pattern. [low]
  Sources: [ITPro on Microsoft Fabric IQ](https://www.itpro.com/technology/artificial-intelligence/fragmentation-is-poison-how-microsoft-is-targeting-disparate-data-to-boost-ai-adoption), [Tom's Guide Build 2026 coverage](https://www.tomsguide.com/news/live/microsoft-build-2026), [Microsoft Copilot connectors overview](https://learn.microsoft.com/en-us/microsoft-365/copilot/connectors/overview), [Microsoft Graph overview](https://learn.microsoft.com/en-us/graph/overview).
  Downgrade: secondary-source reliance; indirectness.

- Finding: Modern enterprise AI context platforms converge on a layered architecture: connectors and source access, graph/catalog/index layers, memory/context selection, tool/action access, security guardrails, and observability. [moderate]
  Sources: [AWS AgentCore Memory](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/memory.html), [AWS AgentCore Gateway](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway.html), [AWS AgentCore Observability](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/observability.html), [Salesforce Agentforce](https://www.salesforce.com/agentforce/), [Glean Enterprise Graph](https://www.glean.com/product/enterprise-graph), [Microsoft Graph overview](https://learn.microsoft.com/en-us/graph/overview), [MCP resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources).
  Downgrade: vendor-blogged; heterogeneity.

- Finding: "Graph as context" is increasingly treated as a derived or learned retrieval surface, not always as a manually curated source-of-record graph. PromptQL describes schema introspection plus a shared operational wiki, Glean describes ML-inferred enterprise and personal graphs, GraphRAG extracts entities/relationships/claims from text, and lightweight research prototypes explore file-based context trees. [moderate]
  Sources: [PromptQL product](https://promptql.io/product), [PromptQL architecture](https://promptql.io/why-promptql-works), [Glean Enterprise Graph](https://www.glean.com/product/enterprise-graph), [Microsoft GraphRAG](https://microsoft.github.io/graphrag/), [ByteRover paper](https://arxiv.org/abs/2604.01599).
  Downgrade: vendor-blogged; heterogeneity; survivorship bias.

- Finding: Operational ontology platforms still emphasize a governed semantic layer that maps real-world entities, relationships, actions, functions, security, and end-user applications, rather than merely indexing documents. [high]
  Sources: [Palantir Ontology overview](https://www.palantir.com/docs/foundry/ontology/overview/), [Palantir Ontology best practices](https://www.palantir.com/docs/foundry/ontology/ontology-best-practices/), [Backstage system model](https://backstage.io/docs/features/software-catalog/system-model/).

- Finding: Software landscape context should start from developer-owned descriptors and contracts where possible. Backstage's model treats components, APIs, resources, systems, and domains as catalog entities and supports human-maintainable YAML descriptors. [high]
  Sources: [Backstage system model](https://backstage.io/docs/features/software-catalog/system-model/), [Backstage descriptor format](https://backstage.io/docs/features/software-catalog/descriptor-format/).

- Finding: Source integration should distinguish synced/indexed connectors from live/federated connectors. Microsoft Copilot connector guidance explicitly separates synced connectors that index data from federated connectors that fetch live data through MCP; this maps cleanly to stable versus living ontology data. [high]
  Sources: [Microsoft Copilot connectors overview](https://learn.microsoft.com/en-us/microsoft-365/copilot/connectors/overview), [Microsoft Graph overview](https://learn.microsoft.com/en-us/graph/overview), [MCP resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources).

- Finding: Agent/tool/action registries are becoming part of the context layer, because agents need not only facts but also a governed view of what they may do. AWS AgentCore Gateway, Salesforce AgentExchange/MCP, Glean MCP Gateway, and MCP tools/resources all point in this direction. [moderate]
  Sources: [AWS AgentCore Gateway](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway.html), [Salesforce Agentforce](https://www.salesforce.com/agentforce/), [Glean Enterprise Graph](https://www.glean.com/product/enterprise-graph), [MCP tools](https://modelcontextprotocol.io/specification/2025-06-18/server/tools), [MCP resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources).
  Downgrade: vendor-blogged; heterogeneity.

- Finding: Human-in-the-loop learning is a core living-context pattern. PromptQL emphasizes wiki-style context updates from work conversations; MCP elicitation standardizes structured user input; Salesforce and AWS emphasize agent testing, supervision, observability, and traces. [moderate]
  Sources: [PromptQL architecture](https://promptql.io/why-promptql-works), [MCP elicitation](https://modelcontextprotocol.io/specification/2025-06-18/client/elicitation), [Salesforce Agentforce](https://www.salesforce.com/agentforce/), [AWS AgentCore Observability](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/observability.html).
  Downgrade: vendor-blogged.

- Finding: Security and authorization are not bolt-ons in the leading patterns. They appear at source access, indexing, retrieval, tool invocation, agent identity, audit, and runtime observation layers. [high]
  Sources: [AWS AgentCore Gateway](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway.html), [PromptQL security](https://promptql.io/security), [Glean Enterprise Graph](https://www.glean.com/product/enterprise-graph), [Salesforce Agentforce](https://www.salesforce.com/agentforce/), [MCP resources security considerations](https://modelcontextprotocol.io/specification/2025-06-18/server/resources).

- Finding: Broad data and metadata cataloging should usually be integrated rather than rebuilt. OpenMetadata's connector catalog spans databases, dashboards, messaging, pipelines, ML, storage, metadata, and search systems, which supports our decision to link or sync from established catalog platforms rather than recreating every connector in the ontology pilot. [moderate]
  Sources: [OpenMetadata connectors](https://docs.open-metadata.org/latest/connectors), [Microsoft Graph overview](https://learn.microsoft.com/en-us/graph/overview).
  Downgrade: indirectness.

## Article Vendor Solution Pattern Check

This pass specifically checked the non-AWS vendors named in the user-supplied VentureBeat article.

| Article Vendor | Article-Named Offering | Publicly Verifiable Source Pattern | Pattern To Fold In | Roadmap Impact |
|----------------|------------------------|------------------------------------|--------------------|----------------|
| Snowflake | Horizon Context and Cortex Sense | Exact names were not found in public primary docs during this pass. Public docs show Horizon Catalog, Cortex Agents, Cortex Analyst, Cortex Search, semantic views, AI guardrails, lineage, object tagging, agent tools, threads, feedback, evaluations, and monitoring. | Treat the pattern as **governed semantic catalog plus agentic workflow**: semantic views define business meaning; search indexes unstructured context; agents plan/use tools/reflect; monitoring/evaluation closes the loop. | Add explicit verified-query/semantic-view examples and agent run feedback as context artifacts. Keep catalog integration as a scale feature unless pilot source links are insufficient. |
| Microsoft | Fabric IQ | Exact public primary docs for Fabric IQ were not found during this pass. Secondary reporting describes Fabric IQ as semantic/ontological context for Fabric, while primary Microsoft docs verify Microsoft Graph, Copilot connectors, synced connectors, federated MCP connectors, ACL-based indexing, continuous sync, Graph Data Connect, and action extensibility. | Treat the pattern as **work graph plus connector mode discipline**: indexed enterprise graph where broad search is useful; live/federated MCP where data is sensitive, dynamic, or source-controlled; actions separated from knowledge retrieval. | Our `connectorMode` field is correct and should remain central. Add ACL/permission mirroring and read-only-by-default connector posture to source/tool profiles. |
| Redis | Redis context platform | Public docs show Redis Iris as a context engine with LangCache semantic cache, Agent Memory, Context Retriever, and live relational data integration; RedisVL supports vector search and semantic caching patterns. | Treat the pattern as **runtime context acceleration**: semantic cache, governed agent memory, generated tools from data models, and low-latency live-sync copies for operational reads. | Keep semantic cache and agent memory deferred. Add cache/memory invalidation, poisoning, access, and freshness tests before scale adoption. |
| Pinecone | Nexus | Exact public primary docs for Pinecone Nexus were not found during this pass. Public docs show Pinecone Assistant: managed document upload, chunking/embedding/storage, metadata filters, grounded answers with citations, context-snippet retrieval, answer evaluation, and per-assistant MCP servers. | Treat the pattern as **compiled document context plus inspectable snippets**: managed context preparation, metadata-filtered retrieval, citations, separate context-snippet API, and answer evaluation. | Our compiled context packet is the right pilot analogue. Add snippet/debug retrieval and answer-evaluation expectations to the pilot artifact and golden tests. |

## Cross-Vendor Pattern Synthesis

| Cross-Vendor Pattern | Snowflake Signal | Microsoft Signal | Redis Signal | Pinecone Signal | Implication For Us |
|----------------------|------------------|------------------|--------------|-----------------|--------------------|
| Semantic layer before generation | Semantic views, business definitions, relationships, verified examples. | Fabric IQ described as semantic/ontological context; Graph relationships provide work context. | Context Retriever defines a data model once and generates tools. | File metadata filters and assistant instructions constrain retrieval. | Add semantic mappings and verified examples to source profiles and context attachments. |
| Connector mode discipline | Catalog-linked databases and cross-engine governance. | Synced connectors versus federated MCP connectors. | Live sync into Redis for fast reads. | Uploaded/managed files plus MCP server exposure. | Keep `connectorMode` as a first-class source-profile field. |
| Runtime action surface | Cortex Agents tools, custom UDFs, MCP connectors, code execution. | Action connectors/plugins separate from read-only connectors. | Generated governed tools from the data model. | Assistant MCP server exposes retrieval as a tool. | Keep agent/tool/action registry separate from source facts. |
| Evaluation and feedback | Threads, feedback, monitoring, evaluations. | Copilot connector relevance and Graph-driven context; secondary reporting emphasizes oversight. | Cache hit/miss and memory behavior need governance. | Answer evaluation and context-snippet retrieval. | Add golden context-packet and answer-evaluation checks; use feedback to propose, not auto-promote. |
| Performance layer | Cortex Search materialization and target lag. | Synced index versus live fetch. | Semantic cache, memory, and live-sync reads. | Managed chunking, embeddings, and snippet budgets. | Defer runtime optimization until pilot proves repeated cost/latency pain. |
| Security and permissions | RBAC, query-engine governance, guardrails. | ACLs in connectors and source permissions for federated connectors. | Access tags for generated tools. | File metadata and temporary signed URLs treated as sensitive. | Add permission mirroring, sensitive-link handling, and read-only-by-default rules. |

## Pattern Inventory

| Pattern | What To Fold In | Reduced Build Impact | Long-Term Impact |
|---------|-----------------|----------------------|------------------|
| Operational ontology | Keep a governed semantic core, relationship model, actions/functions concept, security, and user-facing views. | Keep core Markdown/YAML and gate profiles reviewable. | Add full semantic governance, action governance, graph store, and application surfaces when scale requires it. |
| Shared operational wiki | Let context grow from work corrections, review notes, and decisions, not only from top-down modeling. | Use AI-written, human-reviewed wiki pages with frontmatter and fact states. | Add workflow, review queues, ownership, and freshness dashboards. |
| Developer catalog | Describe software through repo-local descriptors, APIs, resources, systems, domains, ownership, lifecycle, and links. | Prefer `catalog-info.yaml` or `ontology-context.yaml` plus linter. | Integrate with developer portal/catalog and synchronize canonical software graph. |
| Storage/object annotation | Attach context near raw objects or documents before it becomes canonical ontology fact. | Use wiki/document frontmatter and optional source-object metadata; do not require object-store integration for the pilot. | Add object/storage annotation connectors where evidence or document provenance needs stronger automation. |
| Metadata catalog | Delegate data, lineage, BI, pipeline, ML, and storage metadata to existing catalogs/connectors. | Link or lightly sync only pilot-critical metadata. | Add catalog adapters and source authority profiles. |
| Semantic/catalog enrichment | Add business definitions, usage rules, runbooks, query examples, lineage, data quality, and object tags at the catalog layer. | Represent as source-profile fields, wiki pages, and reviewed semantic mappings. | Add catalog semantic enrichment pipeline and catalog-governance integration. |
| Graph-assisted retrieval | Use extracted/derived graph neighborhoods and community summaries to improve multi-hop reasoning. | Build a rebuildable derived graph index, not a governed graph database first. | Add graph store and graph proposal governance after pilot proof. |
| Self-learning graph proposals | Let usage traces and source-result correctness propose relationships, authority weighting, and source-quality changes. | Store proposals in a candidate graph proposal file/queue. | Add graph-learning service, source-correctness scoring, steward review, and promotion workflow. |
| Synced versus federated connectors | Choose indexing for stable/common context and live fetch for sensitive, volatile, or high-volume sources. | Encode retrieval mode in source profiles and avoid over-ingestion. | Build connector framework supporting batch, event, and live/federated access. |
| Task-specific context compilation | Compile task-ready context artifacts before agents query, rather than retrieving every source at runtime. | Treat the Solution Design Brief context packet as the first compiled artifact. | Add context compilation/materialization service for common agent tasks and gates. |
| Semantic cache and agent memory | Cache repeated answers/context and retain useful interaction memory with governance and expiry. | Defer unless repeated pilot queries create measurable latency/cost pain. | Add semantic cache, agent memory store, retention rules, and poisoning/leakage tests. |
| Agent/tool registry | Context selection should include available actions, tool scopes, contracts, and permissions. | Add file-based agent/tool/action manifests and MCP/resource descriptors. | Add governed registry, semantic tool selection, tool audit, and action approval. |
| Trace-driven improvement | Use agent traces, review corrections, and failed retrievals to propose context updates and graph edges. | Generate proposed wiki/manifest updates from traces; keep them candidate until reviewed. | Add trace ingestion, learned edge proposal, confidence scoring, and curation workflows. |
| Guardrails and audit | Enforce authorization, classification, fact state, and evidence requirements before retrieval/action. | Keep access leakage tests and citation/fact-state checks in the pilot. | Add centralized policy, audit store, and runtime observability. |

## Implications For The Reduced Roadmap

The reduced roadmap should stay reduced, but it needs six explicit additions:

1. A file-based agent/tool/action registry for the pilot.
2. Source profiles that explicitly choose synced, federated/live, linked-only, or human-elicited retrieval.
3. A trace-to-context improvement loop that proposes wiki or manifest updates after reviews and failed/weak briefs.
4. A derived graph proposal queue so automated graph discovery can suggest relationships without becoming authoritative.
5. A context-attachment/frontmatter pattern for documents, wiki pages, evidence snapshots, and source-object links.
6. A task-specific context compilation step that makes the Solution Design Brief context packet the first compiled context artifact.

This keeps the pilot lightweight while acknowledging that AI systems are context-selection systems plus governed action surfaces.

## Implications For The Full Roadmap

The full roadmap should add:

1. Agent/tool registry APIs and governance UI.
2. Trace ingestion and context-learning services.
3. Learned relationship proposal and graph curation workflow.
4. Synced/federated connector framework.
5. Runtime policy enforcement across retrieval and action.
6. Cost/context budget optimization as an operational concern.
7. Object/storage annotation adapters for evidence and document provenance.
8. Catalog semantic enrichment pipeline.
9. Context compilation/materialization service.
10. Semantic cache and governed agent memory.
11. Source-correctness and context-usage scoring.

The full roadmap should not assume all graph growth is manual. It should assume graph growth comes from four channels: curated ontology changes, source-system synchronization, document/contract extraction, and agent/human work traces.

## Known Unknowns

- **Known-unknown:** Whether AWS Context, S3 Annotations, Glue skill assets, Snowflake Horizon Context/Cortex Sense, Microsoft Fabric IQ, and Pinecone Nexus have public primary product documentation under those exact names. Would be closed by: official product pages, public docs, release notes, or an AWS/Snowflake/Microsoft/Pinecone announcement.
- **Known-unknown:** Which enterprise systems are already available in the user's environment for catalog, GRC, CMDB, developer portal, wiki, and AI agent runtime. Would be closed by: a source-system inventory and platform constraints review.
- **Known-unknown:** Whether derived graph plus search/vector indexes can answer the pilot's multi-hop design questions without a graph database. Would be closed by: a pilot benchmark using the Phase 11 worked instance and one real product/system slice.
- **Known-unknown:** What level of agent trace capture is allowed under enterprise privacy, legal, and security policy. Would be closed by: security and AI governance review of trace retention, redaction, and access rules.
- **Unknowable:** Which vendor pattern will become dominant long-term. Why not: the context-layer market is still moving quickly, and product capabilities are changing across 2025-2026.

## Research Anchors

- User-supplied VentureBeat article text: "AWS enters the context layer race with a graph that learns from agents, not manual curation," Sean Michael Kerner, published 2026-06-17.
- [AWS S3 Tables](https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-tables.html)
- [AWS Glue Data Catalog](https://docs.aws.amazon.com/glue/latest/dg/components-overview.html)
- [AWS Lake Formation](https://docs.aws.amazon.com/lake-formation/latest/dg/what-is-lake-formation.html)
- [Snowflake Horizon Catalog](https://docs.snowflake.com/en/user-guide/snowflake-horizon)
- [Snowflake Cortex Agents](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-agents)
- [Snowflake Cortex Analyst](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-analyst)
- [Snowflake Cortex Search](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-search/cortex-search-overview)
- [Redis Iris context engine](https://redis.io/docs/latest/develop/ai/context-engine/)
- [RedisVL](https://redis.io/docs/latest/develop/ai/redisvl/)
- [Pinecone Assistant](https://docs.pinecone.io/guides/assistant/overview)
- [Pinecone database overview](https://docs.pinecone.io/guides/get-started/overview)
- [ITPro on Microsoft Fabric IQ](https://www.itpro.com/technology/artificial-intelligence/fragmentation-is-poison-how-microsoft-is-targeting-disparate-data-to-boost-ai-adoption)
- [PromptQL product](https://promptql.io/product)
- [PromptQL architecture](https://promptql.io/why-promptql-works)
- [PromptQL security](https://promptql.io/security)
- [Palantir Ontology overview](https://www.palantir.com/docs/foundry/ontology/overview/)
- [Palantir Ontology best practices](https://www.palantir.com/docs/foundry/ontology/ontology-best-practices/)
- [AWS AgentCore Memory](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/memory.html)
- [AWS AgentCore Gateway](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/gateway.html)
- [AWS AgentCore Observability](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/observability.html)
- [Glean Enterprise Graph](https://www.glean.com/product/enterprise-graph)
- [Salesforce Agentforce](https://www.salesforce.com/agentforce/)
- [Microsoft GraphRAG](https://microsoft.github.io/graphrag/)
- [Microsoft Graph overview](https://learn.microsoft.com/en-us/graph/overview)
- [Microsoft Copilot connectors overview](https://learn.microsoft.com/en-us/microsoft-365/copilot/connectors/overview)
- [MCP resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources)
- [MCP tools](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
- [MCP elicitation](https://modelcontextprotocol.io/specification/2025-06-18/client/elicitation)
- [Backstage system model](https://backstage.io/docs/features/software-catalog/system-model/)
- [Backstage descriptor format](https://backstage.io/docs/features/software-catalog/descriptor-format/)
- [OpenMetadata connectors](https://docs.open-metadata.org/latest/connectors)
- [Semantic MediaWiki](https://www.semantic-mediawiki.org/)
- [ByteRover: Agent-Native Memory Through LLM-Curated Hierarchical Context](https://arxiv.org/abs/2604.01599)
