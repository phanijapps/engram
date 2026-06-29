# Academic Research Findings: AI Agent Memory vs Knowledge

**Purpose**: Web-search-backed academic research on the distinction between memory and knowledge in AI agent systems, covering taxonomies, representation models, and architectural approaches.

---

## 1. Memory Taxonomies for AI Agents

### 1.1 The CoALA Framework

The dominant taxonomy for AI agent memory is the **CoALA (Cognitive Architectures for Language Agents)** framework, formalized by Sumers et al. at Princeton (arXiv:2309.02427, 2023). CoALA decomposes agent cognition into four memory modules directly inspired by human cognitive science, particularly Tulving's trichotomy:

| Module | Description | Function |
|--------|-------------|----------|
| **Working Memory** | Ephemeral context window; active processing buffer | Holds current conversation state, intermediate reasoning |
| **Episodic Memory** | Persistent time-series event records | Logs of what happened, when, with whom; supports "remembering" |
| **Semantic Memory** | Persistent structured knowledge and facts | Durable, organized information about the world; supports "knowing" |
| **Procedural Memory** | Persistent action instructions | How to perform tasks and operations; encodes skills and rules |

Source: [CoALA on arXiv](https://arxiv.org/abs/2309.02427), [CoALA GitHub](https://github.com/ysymyth/awesome-language-agents), [atlan.com](https://atlan.com/know/types-of-ai-agent-memory/)

### 1.2 Functional vs Temporal Taxonomies

Recent scholarship distinguishes between:

- **Temporal taxonomies** (older): Classify memory by time — short-term, long-term.
- **Functional taxonomies** (current): Classify by *what the memory does* — Factual (knowledge), Experiential (insights & skills), and Working Memory (active context management).

Source: [Agent-Memory-Paper-List on GitHub](https://github.com/Shichun-Liu/Agent-Memory-Paper-List)

### 1.3 Hierarchical Memory Patterns

Tiered memory architectures manage content across multiple storage tiers:

- **Tier 1 — Context/Working**: Fixed context window, fast access, volatile
- **Tier 2 — Structured DB**: Semi-structured records (episodic), moderate latency
- **Tier 3 — Vector Store**: Semantic embeddings, approximate nearest-neighbor search
- **Tier 4 — Cold Archive**: Raw logs, documents, high latency

Key exemplar: **MemGPT** (Packer et al., 2024, arXiv:2310.08560) applies virtual context management—drawing inspiration from OS memory management—to move data between fast/slow memory tiers via LLM function calls.

Source: [MemGPT arXiv paper](https://arxiv.org/abs/2310.08560), [Memory for Autonomous LLM Agents survey](https://arxiv.org/html/2603.07670v1)

### 1.4 Classical Cognitive Architectures

Two foundational symbolic architectures precede and inform modern LLM agent designs:

**ACT-R (Adaptive Control of Thought—Rational)**:
- Separates declarative memory (facts/chunks) from procedural memory (production rules)
- Retrieval is probabilistic, governed by activation decay and associative spreading
- Maps to modern agent designs where tool-selection logic (procedural) must be separated from knowledge retrieval (semantic)

**Soar**:
- Production-rule system with learning via chunking
- Working memory holds goals, situation, intermediate results as symbolic graph structures
- Procedural memory encodes skills; semantic and episodic memories handle long-term knowledge

Source: [Soar Wikipedia](https://en.wikipedia.org/wiki/Soar_(cognitive_architecture)), [zylos.ai research](https://zylos.ai/research/2026-03-12-cognitive-architectures-ai-agents-perception-to-action/), [ACT-R analysis on arXiv](https://arxiv.org/abs/2201.09305)

---

## 2. Knowledge Representation Models

### 2.1 Spectrum of Representation Approaches

Knowledge in AI agent systems spans a spectrum from unstructured to highly structured:

| Model | Description | Strength | Weakness |
|-------|-------------|----------|----------|
| **Flat Facts** | Raw text snippets, key-value pairs | Simple, universal | No relationships, no inference |
| **Hierarchical Taxonomies** | Is-a and part-of hierarchies | Structured categorization | Rigid, limited expressiveness |
| **Vector Embeddings** | Dense numerical representations in high-dimensional space | Semantic similarity search | Loses explicit relationships |
| **Knowledge Graphs** | Typed nodes + typed edges + ontologies | Inference, multi-hop reasoning | Maintenance overhead |

Source: [DEV Community article](https://dev.to/sreeni5018/from-rag-to-knowledge-graphs-why-the-agent-era-is-redefining-ai-architecture-3fgc), [zbrain.ai](https://zbrain.ai/knowledge-graphs-for-agentic-ai/)

### 2.2 Vector Embeddings and RAG

**Retrieval-Augmented Generation (RAG)** grounds LLM outputs by retrieving relevant text chunks and injecting them into the prompt at inference. The standard pipeline:

1. Chunk source documents into segments
2. Generate vector embeddings for each chunk
3. Store in a vector database (e.g., Pinecone, Weaviate, Chroma)
4. At query time, perform approximate nearest-neighbor (ANN) search
5. Inject top-k retrieved chunks as context

**Limitation**: RAG retrieves raw text, not structured knowledge. It cannot reason over relationships or perform multi-hop inference.

Source: [GoodData.AI](https://www.gooddata.ai/blog/from-rag-to-graphrag-knowledge-graphs-ontologies-and-smarter-ai/), [Atlan](https://atlan.com/know/knowledge-graphs-vs-rag-for-ai/)

### 2.3 Knowledge Graphs and Ontologies

A **knowledge graph** extends a graph database with a semantic layer:

- **Nodes** represent entities: Person, Organization, Place, Concept, Event
- **Edges** represent typed relationships: works_for, located_in, knows, depends_on
- **Ontologies** define the schema: entity types, valid relationship types, interpretation rules, inference axioms

Ontologies standardize vocabulary and enable reasoning across the graph. They define what relationships are valid and allow agents to infer non-explicit facts.

Source: [zbrain.ai](https://zbrain.ai/knowledge-graphs-for-agentic-ai/), [Enterprise Knowledge](https://enterprise-knowledge.com/ontology-and-knowledge-graph-in-the-age-of-ai-and-agents/), [Neo4j blog](https://neo4j.com/blog/developer/knowledge-graph-structured-semantic-search/)

### 2.4 GraphRAG: Combining Both

**GraphRAG** addresses traditional RAG's limitations through three innovations:
1. Graph-structured knowledge representation that captures entity relationships and domain hierarchies
2. Graph-based retrieval techniques enabling context-preserving knowledge retrieval with multi-hop reasoning
3. Structure-aware knowledge integration algorithms

Source: [GraphRAG Survey on arXiv](https://arxiv.org/html/2501.13958v1), [MachineLearningMastery](https://machinelearningmastery.com/vector-databases-vs-graph-rag-for-agent-memory-when-to-use-which/)

---

## 3. Architectural Approaches

### 3.1 Memory-Augmented Agents

Memory-augmented agents improve decision-making by leveraging causal relationships between actions and outcomes. Key systems:

| System | Approach | Key Feature |
|--------|----------|-------------|
| **MemGPT** | Tiered virtual memory | OS-style paging between context and external storage |
| **Letta** | Three-tier model (core/archival/recall) | Agent controls what to remember/forget via function calls |
| **Mem0** | Scalable long-term memory | Production-ready, hierarchical memory |
| **Zep** | Temporal knowledge graphs | Timestamped entity tracking over agent sessions |
| **Graphiti** | Episodic memory graphs | Event-based temporal knowledge extraction |

Source: [Mem0 paper on arXiv](https://arxiv.org/html/2504.19413v1), [Zylos Research](https://zylos.ai/research/2026-04-05-ai-agent-memory-architectures-persistent-knowledge/), [Letta documentation](https://arxiv.org/html/2606.24775v1)

### 3.2 The CoALA Action Space

Beyond memory modules, CoALA defines a structured **action space** for agent interaction:

- **Internal actions**: Query memory, update memory, reflect/reason
- **External actions**: Interact with tools, APIs, environments, users
- **Decision-making**: Generalized loop selecting actions based on working memory state

This separates the *what the agent knows* (memory modules) from *what the agent does* (action space), enabling systematic agent design.

Source: [AgentPatterns.ai](https://agentpatterns.ai/frameworks/coala-cognitive-architecture-language-agents/), [Paper Without Code](https://paperwithoutcode.com/cognitive-architectures-for-language-agents/)

### 3.3 Symbolic vs Subsymbolic Hybrids

Modern AI agent knowledge systems increasingly combine:

- **Symbolic AI**: Knowledge graphs, ontologies, production rules — interpretable, explicable, supports logical inference
- **Subsymbolic AI**: Neural embeddings, transformer attention — handles noisy data, pattern recognition

This hybrid approach is sometimes called **neural-symbolic AI**. Knowledge graphs provide the scaffold for reasoning; embeddings provide the interface for natural language.

Source: [Neo4j blog](https://neo4j.com/blog/developer/knowledge-graph-structured-semantic-search/), [SmythOS](https://www.smythos.com/developers/agent-architectures/symbolic-ai-and-ontologies/)

### 3.4 Episodic-Semantic Architecture for Long-Horizon Agents

Recent research (arXiv:2605.17625) proposes dual-process memory for scientific agents:

- **Episodic Buffer**: Raw conversational trace, instance-specific, context-preserving
- **Neocortical Memory**: Consolidated knowledge, abstracted and generalized

This mirrors ACT-R's declarative/episodic distinction and enables agents to both remember specific events and generalize from them.

Source: [Episodic-Semantic Architecture on arXiv](https://arxiv.org/html/2605.17625v1)

---

## 4. Key Academic References

| Reference | Citation | URL |
|-----------|----------|-----|
| Cognitive Architectures for Language Agents (CoALA) | Sumers et al., arXiv:2309.02427 | https://arxiv.org/abs/2309.02427 |
| MemGPT: Towards LLMs as Operating Systems | Packer et al., arXiv:2310.08560 | https://arxiv.org/abs/2310.08560 |
| Memory for Autonomous LLM Agents: Mechanisms, Evaluation, and Emerging Frontiers | arXiv:2603.07670 | https://arxiv.org/html/2603.07670v1 |
| Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory | arXiv:2504.19413 | https://arxiv.org/html/2504.19413v1 |
| Anatomy of Agentic Memory: Taxonomy and Empirical Analysis | arXiv:2602.19320 | https://arxiv.org/html/2602.19320v1 |
| The Missing Knowledge Layer in Cognitive Architectures for AI Agents | arXiv:2604.11364 | https://arxiv.org/html/2604.11364v2 |
| Episodic-Semantic Memory Architecture for Long-Horizon Scientific Agents | arXiv:2605.17625 | https://arxiv.org/html/2605.17625v1 |
| A Survey of GraphRAG for Customized LLMs | arXiv:2501.13958 | https://arxiv.org/html/2501.13958v1 |
| Are We Ready For An Agent-Native Memory System? | arXiv:2606.24775 | https://arxiv.org/html/2606.24775v1 |
| A-Mem: Agentic Memory for LLM Agents | arXiv:2502.12110 | https://arxiv.org/pdf/2502.12110 |
| An Analysis and Comparison of ACT-R and Soar | arXiv:2201.09305 | https://arxiv.org/abs/2201.09305 |
| Introduction to the Soar Cognitive Architecture | Laird, arXiv:2205.03854 | https://arxiv.org/pdf/2205.03854 |
| Awesome-Memory-for-Agents (paper list) | TsinghuaC3I | https://github.com/TsinghuaC3I/Awesome-Memory-for-Agents |
| Agent-Memory-Paper-List | Shichun-Liu | https://github.com/Shichun-Liu/Agent-Memory-Paper-List |

---

## 5. Synthesis: Memory vs Knowledge in AI Agents

Drawing from the surveyed literature, the distinction between memory and knowledge in AI agents maps to the following:

| Concept | Memory | Knowledge |
|---------|--------|-----------|
| **Nature** | Process and structure | Content and representation |
| **Analogy** | The filing cabinet + filing process | The contents of the cabinet |
| **Tulving origin** | Episodic (events) + Procedural (skills) | Semantic (facts) |
| **CoALA modules** | Working, Episodic, Procedural | Semantic |
| **Storage** | Can be ephemeral (working) or persistent | Typically persistent |
| **Retrieval** | Active recall, search, reflection | Query, inference, RAG |
| **Updates** | Episodic grows with events; procedural evolves with learning | Semantic updates with new facts |

The four-tier memory taxonomy (working → episodic → semantic → procedural) provides the organizational structure; knowledge representation models (flat facts → vector embeddings → knowledge graphs) provide the content forms. Together they define the memory-knowledge architecture of an AI agent.

Source: Compiled from all sources cited above

---

## 6. Research Gaps and Open Questions

1. **Long-term memory scalability**: How to manage memory that grows indefinitely without degradation in retrieval quality or cost
2. **Memory consistency**: Maintaining coherent world state across episodic updates
3. **Knowledge grounding**: Ensuring semantic memory accurately reflects ground truth
4. **Procedural memory learning**: How agents should autonomously update skills without catastrophic forgetting
5. **Cross-agent memory**: Shared episodic/semantic stores for multi-agent systems

---

## Assumptions

1. The CoALA framework (arXiv:2309.02427) is treated as the canonical reference for memory taxonomy in language model agents, given its widespread adoption in the research community and industry frameworks (Letta, Mem0, LangChain).
2. The distinction between memory (structure/process) and knowledge (content) is treated as analytically useful even though implementations often blur the boundary (e.g., vector databases serve both memory and knowledge functions).
3. Academic literature from 2023–2026 is treated as current and authoritative for this fast-moving field; earlier foundational work (ACT-R, Soar) is included where it directly informs modern architectures.
4. Web search results are treated as representative of the current academic and industry landscape, acknowledging that production systems may have proprietary implementations not publicly documented.
5. The surveyed literature predominantly addresses English-language LLM agents; applicability to multilingual or non-LLM agent systems is assumed but not verified.
