# Memory vs Knowledge Architecture for AI Agents

**Comprehensive Architecture Document**

> **Companion HTML Diagrams**: [`architecture-diagrams.html`](./architecture-diagrams.html) — seven SVG diagrams covering the full architecture. Figures are referenced throughout this document as `[see Figure N]`.
>
> **Companion Conceptual Documents**: [`organizing-memory-and-knowledge.md`](../conceptual-design/organizing-memory-and-knowledge.md) — taxonomy and storage framework. [`contextual-usage-strategy.md`](../conceptual-design/contextual-usage-strategy.md) — retrieval timing strategy.

---

## 1. Executive Summary

Memory and knowledge are **separate but composable concerns** in AI agent architecture. Treating them as a single monolithic store produces systems that are difficult to reason about, scale, and evolve. This document presents a modular, composable architecture that separates them explicitly.

**Memory** is the *process and structure* that manages agent state across time. It answers: What happened? What is happening now? How should I act? Memory subsystems are organized around Tulving's trichotomy—episodic (events), semantic (facts), and procedural (skills)—extended with working memory for active context management, as codified in the CoALA (Cognitive Architectures for Language Agents) framework.

**Knowledge** is the *content and representation* that describes the world. It answers: What is true? What relationships exist? Knowledge subsystems span a spectrum from unstructured text (vector embeddings) to highly structured graphs (typed entities and relationships), with ontologies enabling inference.

The architecture is organized into five composable layers:

| Layer | Role |
|-------|------|
| **Orchestration** | Agent loop, central executive, action selection |
| **Retrieval** | Query routing, ranking, provenance tracking, context composition |
| **Memory Subsystems** | Working, Episodic, Semantic, Procedural — manage state across time |
| **Knowledge Subsystems** | Flat Facts, Taxonomy, Vector Store, Knowledge Graph — represent world content |
| **Storage** | Multi-tier persistence: volatile → structured DB → vector → graph → archive |

Together, these layers define a full cognitive architecture for autonomous long-horizon AI agents, grounded in cited academic research and cognitive science principles.

---

## 2. Academic Research Synthesis

### 2.1 Memory Taxonomies: The CoALA Framework

The dominant taxonomy for AI agent memory is the **CoALA framework** (Sumers et al., Princeton, arXiv:2309.02427, 2023). CoALA decomposes agent cognition into four memory modules directly inspired by human cognitive science and Tulving's trichotomy:

| Module | Function | AI Role |
|--------|----------|---------|
| **Working Memory** | Ephemeral context window; active processing buffer | Holds current conversation state, intermediate reasoning |
| **Episodic Memory** | Persistent time-series event records | Logs of what happened, when, with whom; supports "remembering" |
| **Semantic Memory** | Persistent structured knowledge and facts | Durable, organized information about the world; supports "knowing" |
| **Procedural Memory** | Persistent action instructions | How to perform tasks and operations; encodes skills and rules |

Source: [CoALA on arXiv](https://arxiv.org/abs/2309.02427)

The CoALA framework defines not only memory modules but also a structured **action space** for agent interaction—internal actions (query memory, update memory, reflect/reason) and external actions (interact with tools, APIs, environments, users). This separation ensures that *what the agent knows* (memory modules) is decoupled from *what the agent does* (action space), enabling systematic agent design. Source: [AgentPatterns.ai](https://agentpatterns.ai/frameworks/coala-cognitive-architecture-language-agents/)

### 2.2 Functional vs. Temporal Taxonomies

Recent scholarship distinguishes between two classification approaches:

- **Temporal taxonomies** (older): Classify memory by time — short-term, long-term.
- **Functional taxonomies** (current): Classify by *what the memory does* — Factual (knowledge), Experiential (insights & skills), and Working Memory (active context management).

This functional shift reflects a deeper understanding that storage duration is a property of implementation, not of cognitive role. Source: [Agent-Memory-Paper-List on GitHub](https://github.com/Shichun-Liu/Agent-Memory-Paper-List)

### 2.3 Hierarchical Memory Patterns

Tiered memory architectures manage content across multiple storage tiers, analogous to operating system memory management:

| Tier | Storage | Access | Volatility |
|------|---------|--------|------------|
| **T1 — Context/Working** | Context window | Fastest | Volatile |
| **T2 — Structured DB** | Semi-structured records | Moderate | Persistent |
| **T3 — Vector Store** | Semantic embeddings | Moderate | Persistent |
| **T4 — Cold Archive** | Raw logs, documents | High latency | Persistent |

**MemGPT** (Packer et al., 2024, arXiv:2310.08560) pioneered this approach by applying OS-style virtual memory management to LLM agents—drawing data between fast and slow memory tiers via LLM function calls. This is the architectural pattern that motivates the five-layer design in this document. Source: [MemGPT on arXiv](https://arxiv.org/abs/2310.08560)

Production systems instantiating this pattern include **Letta** (three-tier: core/archival/recall), **Mem0** (hierarchical scalable memory), and **Zep** (temporal knowledge graphs). Source: [Mem0 on arXiv](https://arxiv.org/html/2504.19413v1); [Letta on arXiv](https://arxiv.org/html/2606.24775v1); [Zylos Research](https://zylos.ai/research/2026-04-05-ai-agent-memory-architectures-persistent-knowledge/)

### 2.4 Classical Cognitive Architectures: ACT-R and Soar

Two foundational symbolic architectures precede and inform modern LLM agent designs:

**ACT-R (Adaptive Control of Thought—Rational)** separates declarative memory (facts/chunks) from procedural memory (production rules). Retrieval is probabilistic, governed by activation decay and associative spreading. This maps directly to modern agents where tool-selection logic (procedural) must be separated from knowledge retrieval (semantic). Source: [arXiv:2201.09305](https://arxiv.org/abs/2201.09305)

**Soar** uses a production-rule system with learning via chunking. Working memory holds goals, situation, and intermediate results as symbolic graph structures. Procedural memory encodes skills; semantic and episodic memories handle long-term knowledge. Source: [Soar on arXiv](https://arxiv.org/pdf/2205.03854); [Soar Wikipedia](https://en.wikipedia.org/wiki/Soar_(cognitive_architecture))

### 2.5 Knowledge Representation Models

Knowledge in AI agent systems spans a spectrum from unstructured to highly structured:

| Model | Description | Strength | Weakness |
|-------|-------------|----------|----------|
| **Flat Facts** | Raw text snippets, key-value pairs | Simple, universal | No relationships, no inference |
| **Hierarchical Taxonomies** | Is-a and part-of hierarchies | Structured categorization | Rigid, limited expressiveness |
| **Vector Embeddings** | Dense numerical representations | Semantic similarity search | Loses explicit relationships |
| **Knowledge Graphs** | Typed nodes + typed edges + ontologies | Inference, multi-hop reasoning | Maintenance overhead |

Source: [DEV Community article](https://dev.to/bobur/agent-knowledge-vs-memories-understanding-the-difference-4pgj); [zbrain.ai](https://zbrain.ai/knowledge-graphs-for-agentic-ai/)

### 2.6 Retrieval-Augmented Generation (RAG)

RAG grounds LLM outputs by retrieving relevant text chunks and injecting them into the prompt at inference. The standard pipeline: chunk documents, generate vector embeddings, store in a vector database, perform approximate nearest-neighbor (ANN) search at query time, and inject top-k chunks as context. Source: [GoodData.AI](https://www.gooddata.ai/blog/from-rag-to-graphrag-knowledge-graphs-ontologies-and-smarter-ai/)

**Critical limitation**: RAG retrieves raw text, not structured knowledge. It cannot reason over relationships or perform multi-hop inference. This motivates the knowledge graph subsystem as a complementary representation that captures typed entities and relationships. Source: [Atlan](https://atlan.com/know/knowledge-graphs-vs-rag-for-ai/)

### 2.7 Knowledge Graphs and Ontologies

A **knowledge graph** extends graph storage with a semantic layer: nodes represent entities (Person, Organization, Place, Concept, Event); edges represent typed relationships (works_for, located_in, knows, depends_on); ontologies define the schema, entity types, valid relationship types, and inference axioms. Ontologies standardize vocabulary and enable agents to infer non-explicit facts. Source: [Enterprise Knowledge](https://enterprise-knowledge.com/ontology-and-knowledge-graph-in-the-age-of-ai-and-agents/); [Neo4j blog](https://neo4j.com/blog/developer/knowledge-graph-structured-semantic-search/)

### 2.8 GraphRAG: Composing Vector and Graph

**GraphRAG** addresses traditional RAG's limitations through three innovations: graph-structured knowledge representation capturing entity relationships and domain hierarchies; graph-based retrieval enabling context-preserving multi-hop reasoning; and structure-aware knowledge integration algorithms. Source: [GraphRAG Survey on arXiv](https://arxiv.org/html/2501.13958v1)

This is the neural-symbolic hybrid approach—knowledge graphs provide the scaffold for reasoning; embeddings provide the interface for natural language. Source: [Neo4j blog](https://neo4j.com/blog/developer/knowledge-graph-structured-semantic-search/); [SmythOS](https://www.smythos.com/developers/agent-architectures/symbolic-ai-and-ontologies/)

### 2.9 Episodic-Semantic Dual-Process Architecture

Recent research (arXiv:2605.17625) proposes dual-process memory for long-horizon agents:

- **Episodic Buffer**: Raw conversational trace, instance-specific, context-preserving
- **Neocortical Memory**: Consolidated knowledge, abstracted and generalized

This mirrors ACT-R's declarative/episodic distinction and enables agents to both remember specific events and generalize from them. Source: [Episodic-Semantic Architecture on arXiv](https://arxiv.org/html/2605.17625v1)

---

## 3. Philosophical and Cognitive Science Context

Human cognitive science provides the foundational metaphors that shape this architecture's design. The memory-knowledge distinction in AI agents maps directly to distinctions established by decades of research in psychology and philosophy of mind.

### 3.1 Tulving's Episodic/Semantic Memory Distinction

Endel Tulving's 1972 framework remains the foundational reference for understanding memory architecture. He proposed two distinct but interdependent long-term memory systems:

**Semantic Memory** — General knowledge about the world: facts, concepts, meanings, vocabulary. Associated with noetic consciousness ("knowing") — a sense of familiarity without recollection of origin. Time-independent: facts exist outside personal temporal experience. Example: knowing that Paris is the capital of France.

**Episodic Memory** — Memory for personally experienced events situated in time and space. Associated with autonoetic consciousness ("remembering") — the felt sense of mentally traveling back to re-experience an event. Self-referential: memories are experienced as "happening to me." Example: remembering your last birthday dinner.

Tulving later refined this (1985, 2002) to emphasize that episodic memory depends on semantic knowledge for encoding and retrieval, while semantic memory itself may be built from accumulated episodic experiences. This interdependence is central to his **SPI model** (Serial, Parallel, Independent encoding): *"Episodic memory, by its nature, requires semantic memory for its operation, while semantic memory may be independent of episodic memory."* Source: [PMC2952732](https://pmc.ncbi.nlm.nih.gov/articles/PMC2952732/)

### 3.2 Remembering vs. Knowing

Tulving's (1985) phenomenological distinction between retrieval states is directly relevant to AI provenance tracking:

**Remembering** (episodic retrieval): Autonoetic consciousness — vivid re-experiencing of the original event. Subjective feeling of mentally traveling through time. Associated with episodic memory retrieval.

**Knowing** (semantic retrieval): Noetic consciousness — sense of familiarity without re-experiencing. Abstract knowledge without episodic detail. Associated with semantic memory retrieval.

This distinction suggests that AI agents should track **source attribution**: how was this information retrieved (direct experience vs. learned knowledge), and calibrate confidence accordingly. Source: [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S1053810009000804)

### 3.3 Baddeley's Working Memory Model

Alan Baddeley and Graham Hitch's (1974) model describes working memory as a multi-component active processing system:

| Component | Function | AI Mapping |
|-----------|----------|------------|
| **Central Executive** | Attentional controller; directs resources between subsystems | Orchestration layer |
| **Phonological Loop** | Stores verbal/auditory information (1-2 second decay) | Context window management |
| **Visuospatial Sketchpad** | Stores visual/spatial information | Structured data representations |
| **Episodic Buffer** (added 2000) | Multidimensional storage integrating multiple sources | Context composer in retrieval layer |

The episodic buffer is particularly relevant: it "integrates information from multiple sources" and "binds information into coherent episodes," providing the architectural model for the retrieval layer's context composer. Source: [Wikipedia — Baddeley's Model](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory)

Working memory has **limited capacity** — Miller's 7±2 chunks. This maps to context window constraints in LLMs, motivating the eviction mechanism that bridges working memory and episodic storage.

### 3.4 Predictive Processing and Free Energy Principle

Karl Friston's free energy principle offers a unifying framework for proactive context management:

**Core insight**: The brain continuously generates predictions (generative models) about sensory inputs. Prediction errors signal mismatches between expected and actual input. Learning involves updating models to minimize prediction error. Two mechanisms operate: **active inference** (acting to confirm predictions) and **perceptual inference** (updating beliefs). Source: [Predictably Correct Substack](https://predictablycorrect.substack.com/p/a-predictive-processing-perspective)

For AI architecture, this translates to maintaining **expectation models** about what context is relevant. When predictions are violated (prediction error), this acts as a surprise signal triggering proactive context retrieval. Higher levels of the memory hierarchy encode abstract predictions; lower levels encode concrete ones—mirroring the episodic → semantic → procedural hierarchy.

### 3.5 Embodied Cognition

Lakoff and Johnson's embodied cognition theory challenges the view of cognition as abstract symbol manipulation:

**Key principle**: Concepts are grounded in bodily experiences. Abstract thought relies on metaphorical extensions from concrete, embodied knowledge. *"If human experience is intricately bound up with large-scale metaphors, and both experience and metaphor are shaped up by the kinds of bodies we have that mediate between agent and world."* Source: [Stanford Encyclopedia of Philosophy](https://plato.stanford.edu/entries/embodied-cognition/)

For AI agents, this suggests benefit from **situated context** — grounding abstract reasoning in concrete, environmental interaction traces. Even language models benefit from sensorimotor-like experience records: the agent's history of interactions with tools, APIs, and environments constitutes its "embodied experience."

### 3.6 Memory Consolidation and Schema Theory

Memory consolidation integrates new episodes into established cognitive schemas:

- **Schemas** are organized knowledge structures that frame new information. Existing schemas accelerate learning of congruent information.
- The **consolidation process** involves a hippocampal-neocortical binding process incorporating newly acquired information into existing cognitive schemas. Source: [Frontiers in Human Neuroscience](https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2023.1217093/full)
- **Context binding**: Episodic memories bind together item information, temporal context, and spatial context. The hippocampus performs this binding, creating integrated memory traces that serve as retrieval cues.

The architectural implication is a **consolidation pipeline**: periodic integration of episodic records into semantic structures, and then into the knowledge graph. This mirrors the hippocampal-neocortical consolidation process described in cognitive science.

---

## 4. Modular Composable Architecture

### 4.1 Architectural Overview

The architecture separates memory (process/structure) from knowledge (content/representation) as two independent axes, each with its own subsystems. They compose through the retrieval layer.

```
┌──────────────────────────────────────────────────────────────────┐
│                    ORCHESTRATION LAYER                            │
│              (Agent Loop / Central Executive)                     │
├──────────────────────────────────────────────────────────────────┤
│                      RETRIEVAL LAYER                              │
│         (Query Router · Ranker · Provenance · Composer)           │
├──────────────────────────┬───────────────────────────────────────┤
│    MEMORY SUBSYSTEMS     │         KNOWLEDGE SUBSYSTEMS           │
│                          │                                        │
│  Working Memory          │  Flat Facts Store                      │
│  Episodic Memory         │  Hierarchical Taxonomy                 │
│  Semantic Memory         │  Vector Embedding Store (RAG)          │
│  Procedural Memory       │  Knowledge Graph                       │
├──────────────────────────┴───────────────────────────────────────┤
│                       STORAGE LAYER                               │
│  (Volatile · Structured DB · Vector Store · Graph Store · Archive)│
└──────────────────────────────────────────────────────────────────┘
```

See [Figure 1 — Component Architecture Diagram](architecture-diagrams.html#d1) for the full SVG visualization of this five-layer structure.

### 4.2 Memory Subsystems

Memory subsystems manage **agent state across time** — what happened, what is happening now, and how to act.

#### Working Memory

| Aspect | Definition |
|--------|------------|
| **Responsibility** | Hold active context for in-process reasoning: current conversation, task state, intermediate results. The only ephemeral tier. |
| **Inputs** | Incoming user messages, tool outputs, retrieved context from the retrieval layer. |
| **Outputs** | Active context window fed to the LLM; eviction signals when capacity is exceeded. |
| **Persistence** | Volatile — lost when session ends. |
| **Capacity** | Bounded by context window size (Miller's 7±2 chunks). Source: [Wikipedia — Baddeley's Model](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory) |
| **Interface** | `read_context()` · `write_fragment(fragment)` · `evict(n_fragments)` |

Working memory is volatile by definition — *"the model forgets everything between runs so the memory has to be on disk and not in the context."* Source: [Loop Engineering video](https://youtu.be/GrNbuWWJYiI) The eviction operation is the bridge to episodic memory: when the context window fills, older fragments must be archived before new content can enter. This mirrors MemGPT's OS-style paging. Source: [MemGPT on arXiv](https://arxiv.org/abs/2310.08560)

#### Episodic Memory

| Aspect | Definition |
|--------|------------|
| **Responsibility** | Store timestamped event records — what happened, when, and in what context. Supports "remembering." |
| **Inputs** | Archived working memory fragments (eviction), session events (tool calls, user turns, agent actions). |
| **Outputs** | Retrieved event sequences matching temporal or contextual queries. |
| **Persistence** | Persistent — time-series store. |
| **Interface** | `record_event(event)` · `recall(query)` · `consolidate()` |

Episodic memory preserves full binding context (time, source, task) as recommended by schema theory — *"episodic memories bind together: item information, temporal context, spatial context."* Source: [Frontiers in Human Neuroscience](https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2023.1217093/full) The `consolidate()` operation is a first-class interface: periodically extracting generalized knowledge from accumulated episodes and writing it to semantic memory. This mirrors the hippocampal-neocortical consolidation process and the dual-process episodic-neocortical architecture for long-horizon agents. Source: [arXiv:2605.17625](https://arxiv.org/html/2605.17625v1)

#### Semantic Memory

| Aspect | Definition |
|--------|------------|
| **Responsibility** | Store durable, structured facts and knowledge about the world. Supports "knowing." |
| **Inputs** | Consolidated facts from episodic memory, explicit fact injection, knowledge base updates. |
| **Outputs** | Fact lookups, structured knowledge for grounding responses, ontology queries. |
| **Persistence** | Persistent — structured database and/or knowledge graph. |
| **Interface** | `store_fact(fact)` · `query(predicate)` · `update_fact(fact_id, update)` · `resolve_entity(entity)` |

Semantic memory bridges memory and knowledge—it is the *memory subsystem* that stores *knowledge content*. Implementations often blur this boundary: vector databases serve both memory and knowledge functions. Source: [academic-research-findings.md §5](./data/academic-research-findings.md) In this architecture, semantic memory is the process interface for managing durable knowledge; the knowledge subsystems provide the storage representations it draws upon.

#### Procedural Memory

| Aspect | Definition |
|--------|------------|
| **Responsibility** | Store action instructions — how to perform tasks. Encodes skills, tool-selection logic, behavioral routines. |
| **Inputs** | Learned action patterns from successful task completions, explicitly authored skill definitions, tool-call schemas. |
| **Outputs** | Applicable action templates / production rules for the current situation. |
| **Persistence** | Persistent — rule store or skill registry. |
| **Interface** | `register_skill(skill)` · `match_action(state)` · `update_skill(skill_id, update)` |

Procedural memory is separated from semantic memory following ACT-R's declarative/procedural split — *"tool-selection logic (procedural) must be separated from knowledge retrieval (semantic)."* Source: [arXiv:2201.09305](https://arxiv.org/abs/2201.09305) This separation ensures that *what the agent knows* (semantic) is decoupled from *what the agent does* (procedural), enabling independent evolution.

See [Figure 2 — Memory Subsystem Diagram](architecture-diagrams.html#d2) for the interaction map between all four memory subsystems.

### 4.3 Knowledge Subsystems

Knowledge subsystems manage **how information about the world is represented and retrieved**. They are content stores that the memory subsystems (particularly semantic memory) draw upon. The four representation models span a spectrum from unstructured to highly structured. Source: [academic-research-findings.md §2.1](./data/academic-research-findings.md)

#### Flat Facts Store

Simple key-value or text-snippet storage with no structural relationships. Strength: simple, universal, low overhead. Weakness: no relationships, no inference capability. Interface: `put(key, value)` · `get(key)` · `search(substring)`. Source: [DEV Community](https://dev.to/bobur/agent-knowledge-vs-memories-understanding-the-difference-4pgj)

#### Hierarchical Taxonomy

Is-a and part-of hierarchies for structured categorization. Strength: organized navigation. Weakness: rigid, limited expressiveness. Interface: `add_concept(concept, parent)` · `get_ancestors(concept)` · `get_children(concept)`.

#### Vector Embedding Store (RAG)

Dense numerical embeddings of text for semantic similarity search. Powers Retrieval-Augmented Generation. The standard pipeline: chunk source documents, generate vector embeddings, store in a vector database, perform approximate nearest-neighbor (ANN) search at query time, inject top-k retrieved chunks as context. Strength: semantic similarity across unstructured text. Weakness: loses explicit relationships, cannot perform multi-hop inference. Interface: `embed_and_store(text, metadata)` · `similarity_search(query_text, k)`. Source: [GoodData.AI](https://www.gooddata.ai/blog/from-rag-to-graphrag-knowledge-graphs-ontologies-and-smarter-ai/)

#### Knowledge Graph

Typed entities connected by typed relationship edges, governed by an ontology schema. Supports inference and multi-hop reasoning. Strength: inference, multi-hop reasoning, explainability. Weakness: maintenance overhead, requires ontology engineering. Interface: `add_entity(entity)` · `add_relationship(subject, predicate, object)` · `traverse(start, path_pattern)` · `infer(query)`. Source: [zbrain.ai](https://zbrain.ai/knowledge-graphs-for-agentic-ai/)

See [Figure 3 — Knowledge Subsystem Diagram](architecture-diagrams.html#d3) for the full visualization of the knowledge representation spectrum and the GraphRAG composition pattern.

### 4.4 Retrieval Layer

The retrieval layer sits between the memory/knowledge subsystems and the orchestration layer. It is the **integration point**—analogous to Baddeley's episodic buffer, which "integrates information from multiple sources" and "binds information into coherent episodes." Source: [Wikipedia — Baddeley's Model](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory)

| Component | Responsibility | Interface |
|-----------|---------------|-----------|
| **Query Router** | Determine which subsystem(s) to query based on query type (temporal → episodic; factual → semantic; skill → procedural; similarity → vector; relational → graph) | `route(query) → SubsystemTarget[]` |
| **Ranker** | Score and order retrieved results by relevance, recency, and confidence. Merge results from multiple subsystems. | `rank(results, query) → RankedResults` |
| **Provenance Tracker** | Attach source attribution: which subsystem, what source, when stored, retrieval confidence | `annotate(result) → ResultWithProvenance` |
| **Context Composer** | Assemble the final context payload, respecting capacity constraints | `compose(ranked, budget) → ContextPayload` |
| **Predictive Retrieval** | *(optional)* Generate expectation-based retrieval hints; trigger proactive loading on prediction error | `predict_context(state) → RetrievalHints` |

See [Figure 4 — Retrieval Layer Data Flow](architecture-diagrams.html#d4) for the full retrieval pipeline visualization.

### 4.5 Composition Patterns

Subsystems are independent modules that assemble into coherent agent configurations. Six patterns are defined, ranging from minimal to full cognitive architecture:

| Pattern | Composition | Use Case |
|---------|-------------|----------|
| **A — Minimal Context** | Working Memory ⇄ Episodic Memory | Stateless assistants needing conversation history |
| **B — Knowledge-Grounded** | Working + Semantic + Vector Store (RAG) | Document Q&A, knowledge-base assistants |
| **C — Skill-Augmented** | Pattern B + Procedural Memory | Agents that both know facts and perform actions |
| **D — Full Cognitive** | All 4 Memory + All 4 Knowledge subsystems | Autonomous long-horizon agents |
| **E — GraphRAG** | Vector Store + Knowledge Graph (composed) | Multi-hop reasoning over structured corpora |
| **F — Consolidation Pipeline** | Episodic → Semantic → Knowledge Graph | Agents that learn from accumulated experience |

See [Figure 5 — Composition Pattern Diagrams](architecture-diagrams.html#d5) for all six patterns visualized.

---

## 5. In-Depth Implementation Details

### 5.1 Component Interfaces

The architecture specifies subsystem interfaces at the **architectural level** (inputs, outputs, responsibilities), not the implementation level (data schemas, serialization formats, query languages). This boundary enables multiple implementations of each subsystem.

**Working Memory Interface**:
- `read_context() → ContextWindow`: Returns the current active context window.
- `write_fragment(fragment: ContextFragment)`: Adds a new fragment to working memory.
- `evict(n_fragments: int) → ArchivedFragments`: Evicts the oldest N fragments, returning them for archival to episodic memory. Triggered when capacity is exceeded.

**Episodic Memory Interface**:
- `record_event(event: Event)`: Stores a timestamped event with full binding context (time, source, task).
- `recall(query: TimeQuery | ContextQuery) → Event[]`: Retrieves events matching a temporal or contextual query.
- `consolidate() → SemanticFacts`: Extracts generalized knowledge from accumulated events and returns facts for semantic memory ingestion.

**Semantic Memory Interface**:
- `store_fact(fact: Fact)`: Persists a durable fact.
- `query(predicate: Predicate) → Fact[]`: Retrieves facts matching a predicate.
- `update_fact(fact_id: ID, update: FactUpdate)`: Modifies an existing fact.
- `resolve_entity(entity: Entity) → EntityNode`: Resolves an entity reference to a knowledge graph node.

**Procedural Memory Interface**:
- `register_skill(skill: Skill)`: Registers a new skill or action pattern.
- `match_action(state: AgentState) → ActionTemplate[]`: Returns applicable action templates for the current state (analogous to ACT-R production-rule firing).
- `update_skill(skill_id: ID, update: SkillUpdate)`: Modifies an existing skill.

**Knowledge Graph Interface**:
- `add_entity(entity: EntityNode)`: Adds a typed entity node.
- `add_relationship(subject, predicate, object)`: Adds a typed relationship edge.
- `traverse(start: Node, path_pattern: PathPattern) → Subgraph`: Performs multi-hop traversal.
- `infer(query: Query) → InferredFact[]`: Executes ontology-based inference.

### 5.2 Data Flow: End-to-End Agent Cycle

The following flow traces a single agent cycle through the architecture:

1. **Perception**: A user message or environment signal enters **Working Memory** via the orchestration layer's agent loop.
2. **Prediction** *(optional, predictive mode)*: The **predictive retrieval** component generates expectation-based retrieval hints from the current agent state. If the incoming input violates expectations (prediction error), a surprise signal triggers proactive context loading. Source: [Predictably Correct Substack](https://predictablycorrect.substack.com/p/a-predictive-processing-perspective)
3. **Retrieval**: The **Query Router** determines which subsystems to query, fanning out in parallel:
   - Temporal/contextual queries → **Episodic Memory** (`recall()`)
   - Factual queries → **Semantic Memory** (`query()`) → **Vector Store** and/or **Knowledge Graph**
   - Action/skill queries → **Procedural Memory** (`match_action()`)
4. **Ranking & Provenance**: The **Ranker** scores results by relevance, recency, and confidence. The **Provenance Tracker** attaches source attribution—tracking *how* information was retrieved (direct experience vs. learned knowledge)—following Tulving's remember/know distinction. Source: [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S1053810009000804)
5. **Context Composition**: The **Context Composer** assembles the final payload within working memory's capacity budget, analogous to Baddeley's episodic buffer binding information from multiple sources. Source: [Wikipedia — Baddeley's Model](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory)
6. **Reasoning & Action**: The **agent loop** processes the composed context and selects an action—either an internal action (reason, reflect) or an external action (tool call, API interaction, user response). Source: [CoALA on arXiv](https://arxiv.org/abs/2309.02427)
7. **Recording**: The action and its outcome are recorded as an event in **Episodic Memory** (`record_event()`).
8. **Consolidation** *(periodic)*: Accumulated episodic events are consolidated into **Semantic Memory** facts via `consolidate()`, which may then be structured into the **Knowledge Graph**.
9. **Skill Learning** *(on task success)*: Successful action patterns are registered in **Procedural Memory** (`register_skill()`), closing the learning loop.

See [Figure 7 — End-to-End Agent Cycle](architecture-diagrams.html#d7) for the complete cycle visualization.

### 5.3 Storage Models

The storage layer maps subsystems to concrete persistence technologies. No single storage engine serves all subsystems; the architecture specifies five tiers, consistent with MemGPT's tiered virtual memory model. Source: [MemGPT on arXiv](https://arxiv.org/abs/2310.08560)

| Tier | Volatility | Latency | Serves Subsystem(s) | Technology Examples |
|------|-----------|---------|---------------------|---------------------|
| **T1 — Context/Working** | Volatile | Fastest | Working Memory | In-process context window, in-memory state |
| **T2 — Structured DB** | Persistent | Moderate | Episodic, Semantic, Flat Facts | PostgreSQL, SQLite, document stores |
| **T3 — Vector Store** | Persistent | Moderate | Vector Embeddings (RAG), Semantic (similarity) | Pinecone, Weaviate, Chroma |
| **T4 — Graph Store** | Persistent | Moderate | Knowledge Graph | Neo4j, property graphs |
| **T5 — Cold Archive** | Persistent | High | Episodic (raw logs), Procedural (historical) | Object storage, log archives |

See [Figure 6 — Storage Tier Diagram](architecture-diagrams.html#d6) for the MemGPT-style paging model visualization.

**Design rationale**: The multi-tier model follows MemGPT's insight that OS-style paging between context and external storage is necessary because working memory is bounded but agents accumulate state indefinitely. Source: [MemGPT on arXiv](https://arxiv.org/abs/2310.08560) Letta's three-tier model (core/archival/recall) and Zep's temporal knowledge graphs are production exemplars. Source: [Letta on arXiv](https://arxiv.org/html/2606.24775v1); [Zylos Research](https://zylos.ai/research/2026-04-05-ai-agent-memory-architectures-persistent-knowledge/)

### 5.4 Retrieval Strategies

| Strategy | When Used | Mechanism |
|----------|-----------|-----------|
| **Temporal recall** | Session continuity, event replay | Episodic memory query by time range or session ID |
| **Semantic similarity** | Fact grounding, document Q&A | Vector embedding ANN search |
| **Relational traversal** | Multi-hop reasoning, entity exploration | Knowledge graph path pattern matching |
| **Pattern matching** | Action selection, skill invocation | Procedural memory production-rule firing |
| **Direct lookup** | Known-key retrieval | Flat facts store or semantic memory predicate query |
| **Predictive loading** | Proactive context preparation | Expectation model + prediction-error surprise signals |

### 5.5 Consolidation Process

Consolidation is the scheduled process of extracting generalized knowledge from accumulated episodic events and integrating it into semantic memory and the knowledge graph:

1. **Extraction**: Scan episodic memory for recurring patterns, successful action sequences, and factual claims implicit in events.
2. **Generalization**: Abstract specific events into general rules or facts (e.g., "user asks for code review on Tuesdays" → a pattern fact).
3. **Conflict resolution**: Check new facts against existing semantic memory for contradictions.
4. **Integration**: Write validated facts to semantic memory. Optionally structure them as knowledge graph entities and relationships.
5. **Pruning**: Optionally archive or discard episodic events that have been successfully consolidated, freeing storage.

This mirrors the hippocampal-neocortical consolidation process: *"the consolidation process involves a hippocampal-neocortical binding process incorporating newly acquired information into existing cognitive schemata."* Source: [Frontiers in Human Neuroscience](https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2023.1217093/full)

---

## 6. Cross-References to HTML Diagrams

The companion file [`architecture-diagrams.html`](./architecture-diagrams.html) provides seven SVG-based diagrams that visualize different aspects of this architecture:

| Figure | Topic | Relevant Sections |
|--------|-------|-------------------|
| [Figure 1](architecture-diagrams.html#d1) | Five-layer component architecture | §4.1 |
| [Figure 2](architecture-diagrams.html#d2) | Memory subsystems and interfaces | §4.2 |
| [Figure 3](architecture-diagrams.html#d3) | Knowledge representation spectrum | §4.3 |
| [Figure 4](architecture-diagrams.html#d4) | Retrieval layer data flow | §4.4, §5.2 |
| [Figure 5](architecture-diagrams.html#d5) | Six composition patterns | §4.5 |
| [Figure 6](architecture-diagrams.html#d6) | Storage tier paging model | §5.3 |
| [Figure 7](architecture-diagrams.html#d7) | End-to-end agent cycle | §5.2 |

These diagrams are referenced at the appropriate points throughout this document. They are HTML/SVG files with no external image dependencies and render in any modern browser.

---

## 7. Assumptions

1. **CoALA as canonical taxonomy**: The CoALA framework (Sumers et al., arXiv:2309.02427) is treated as the authoritative reference for AI agent memory taxonomy, given its widespread adoption in research and industry (Letta, Mem0, LangChain). The four-tier model (working, episodic, semantic, procedural) is adopted directly.

2. **Memory/knowledge as analytically distinct at the interface level**: The separation of memory subsystems from knowledge subsystems is treated as architecturally useful even though implementations blur the boundary (e.g., vector databases serve both memory and knowledge functions, and semantic memory is both a memory subsystem and a knowledge container). The separation is at the *interface* level, not the storage level.

3. **Cognitive science as design metaphor, not identity**: Findings from human cognitive science (Tulving, Baddeley, Friston, schema theory) are used as *design metaphors and principles* for AI architecture. No claim is made that AI agents replicate biological cognition. The mappings (e.g., "context window = working memory") are architectural analogies.

4. **Interface-level specification only**: This document defines subsystem *interfaces* (inputs, outputs, responsibilities) and composition patterns. It does not specify implementation details (data schemas, serialization formats, specific database query languages) — those are implementation-level decisions beyond the scope of architecture design.

5. **Single-agent scope**: The architecture addresses a single agent's memory/knowledge system. Multi-agent memory sharing, distributed consistency, and federation are out of scope and flagged as a research gap.

6. **Storage technology examples are illustrative**: Named technologies (PostgreSQL, Pinecone, Neo4j, etc.) are examples of storage classes, not normative recommendations. Any technology matching the described tier characteristics is architecturally valid.

7. **Retrieval layer described synchronously**: The data flow describes retrieval as a synchronous step in the agent cycle. In practice, predictive retrieval may operate asynchronously; the architecture permits both modes but the primary flow is described synchronously for clarity.

8. **Consolidation scheduling policy unspecified**: The research establishes that consolidation happens but does not specify optimal scheduling policies (time-based, event-count-based, or prediction-error-triggered). This is a known gap.

9. **Procedural memory update policy unspecified**: The `update_skill()` interface is defined but its update policy (how agents should autonomously update skills without catastrophic forgetting) is not specified. This is a known open research question from the academic literature.

10. **No quantitative performance targets**: No quantitative benchmarks for tier capacities, retrieval latencies, or consolidation throughput are available from the cited research. All such targets are omitted to avoid fabricating quantitative claims.

---

## 8. References

| # | Source | Citation | URL |
|---|--------|----------|-----|
| 1 | CoALA Framework | Sumers et al., arXiv:2309.02427 (2023) | https://arxiv.org/abs/2309.02427 |
| 2 | MemGPT | Packer et al., arXiv:2310.08560 (2024) | https://arxiv.org/abs/2310.08560 |
| 3 | Memory for Autonomous LLM Agents Survey | arXiv:2603.07670 | https://arxiv.org/html/2603.07670v1 |
| 4 | Mem0: Scalable Long-Term Memory | arXiv:2504.19413 | https://arxiv.org/html/2504.19413v1 |
| 5 | Anatomy of Agentic Memory | arXiv:2602.19320 | https://arxiv.org/html/2602.19320v1 |
| 6 | The Missing Knowledge Layer in Cognitive Architectures | arXiv:2604.11364 | https://arxiv.org/html/2604.11364v2 |
| 7 | Episodic-Semantic Memory for Long-Horizon Scientific Agents | arXiv:2605.17625 | https://arxiv.org/html/2605.17625v1 |
| 8 | GraphRAG Survey | arXiv:2501.13958 | https://arxiv.org/html/2501.13958v1 |
| 9 | Are We Ready For An Agent-Native Memory System? | arXiv:2606.24775 | https://arxiv.org/html/2606.24775v1 |
| 10 | ACT-R Analysis | arXiv:2201.09305 | https://arxiv.org/abs/2201.09305 |
| 11 | Soar Cognitive Architecture | Laird, arXiv:2205.03854 | https://arxiv.org/pdf/2205.03854 |
| 12 | Tulving SPI Model | PMC2952732 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2952732/ |
| 13 | Tulving Remembering vs. Knowing | ScienceDirect | https://www.sciencedirect.com/science/article/abs/pii/S1053810009000804 |
| 14 | Baddeley's Working Memory Model | Wikipedia | https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory |
| 15 | Predictive Processing / Free Energy | Predictably Correct Substack | https://predictablycorrect.substack.com/p/a-predictive-processing-perspective |
| 16 | Embodied Cognition | Stanford Encyclopedia of Philosophy | https://plato.stanford.edu/entries/embodied-cognition/ |
| 17 | Memory Consolidation and Schemas | Frontiers in Human Neuroscience (2023) | https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2023.1217093/full |
| 18 | Agent Knowledge vs Memories | DEV Community | https://dev.to/bobur/agent-knowledge-vs-memories-understanding-the-difference-4pgj |
| 19 | From RAG to Knowledge Graphs | DEV Community | https://dev.to/sreeni5018/from-rag-to-knowledge-graphs-why-the-agent-era-is-redefining-ai-architecture-3fgc |
| 20 | Knowledge Graphs for Agentic AI | zbrain.ai | https://zbrain.ai/knowledge-graphs-for-agentic-ai/ |
| 21 | Ontology and Knowledge Graph in the Age of AI | Enterprise Knowledge | https://enterprise-knowledge.com/ontology-and-knowledge-graph-in-the-age-of-ai-and-agents/ |
| 22 | Knowledge Graph Structured Semantic Search | Neo4j Blog | https://neo4j.com/blog/developer/knowledge-graph-structured-semantic-search/ |
| 23 | From RAG to GraphRAG | GoodData.AI | https://www.gooddata.ai/blog/from-rag-to-graphrag-knowledge-graphs-ontologies-and-smarter-ai/ |
| 24 | Knowledge Graphs vs RAG for AI | Atlan | https://atlan.com/know/knowledge-graphs-vs-rag-for-ai/ |
| 25 | Symbolic AI and Ontologies | SmythOS | https://www.smythos.com/developers/agent-architectures/symbolic-ai-and-ontologies/ |
| 26 | Soar Cognitive Architecture | Wikipedia | https://en.wikipedia.org/wiki/Soar_(cognitive_architecture) |
| 27 | AI Agent Memory Architectures Survey | Zylos Research | https://zylos.ai/research/2026-04-05-ai-agent-memory-architectures-persistent-knowledge/ |
| 28 | Loop Engineering with Memory and Context | YouTube | https://youtu.be/GrNbuWWJYiI |
| 29 | CoALA Cognitive Architecture Framework | AgentPatterns.ai | https://agentpatterns.ai/frameworks/coala-cognitive-architecture-language-agents/ |
| 30 | Agent-Memory-Paper-List | GitHub / Shichun-Liu | https://github.com/Shichun-Liu/Agent-Memory-Paper-List |