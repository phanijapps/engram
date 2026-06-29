# Modular & Composable Memory/Knowledge Architecture for AI Agents (v2)

**Purpose**: This is the second iteration of the modular, composable memory/knowledge architecture. It extends the v1 design ([architecture-design.md](./architecture-design.md)) with new insights from hierarchical memory research (ACT-R, Schema Theory), dynamic taxonomy evolution (Ontology Evolution, OntoDrift), and hierarchical knowledge representation (W3C SKOS). Memory and knowledge remain **separate but composable concerns**: memory is the *process and structure* that manages state; knowledge is the *content and representation* that describes the world.

> **Version note**: Changes from v1 are marked with **[v2]**. All v1 components remain backwards-compatible — v2 additions are extensions, not replacements.

> **Companion documents**: For the conceptual taxonomy, see [Organizing Memory and Knowledge](../../conceptual-design/organizing-memory-and-knowledge.md). For retrieval timing strategy, see [Contextual Usage Strategy](../../conceptual-design/contextual-usage-strategy.md). For hierarchical taxonomy research, see [hierarchical-taxonomy-research.md](./hierarchical-taxonomy-research.md).

---

## 1. Executive Summary (v2)

This architecture defines a modular, composable memory/knowledge system for AI agents. **v2** introduces five major enhancements over the baseline v1 design:

| Enhancement | Source | Key Change |
|-------------|--------|------------|
| **Hierarchical Memory** | ACT-R, Schema Theory, Temporal Trees | Chunk hierarchy within episodic memory; cue-based retrieval; dual-index (temporal + hierarchical); construction/navigation separation |
| **Dynamic Taxonomies** | Ontology Evolution research | 4-phase evolution pipeline (Discovery → Proposal → Validation → Merge); semantic drift detection; provenance tracking; concept formation pipeline |
| **Hierarchical Taxonomies (SKOS)** | W3C SKOS Standard | SKOS-aligned schema; broader/narrower relations; collections for facets; cross-scheme mapping; URI-based concepts |
| **Enhanced Retrieval Layer** | ACT-R cue-based retrieval | Cue-based retrieval interface; multi-dimensional indexing (temporal, hierarchical, semantic); hierarchical query routing |
| **Updated Composition Patterns** | Hierarchical memory + SKOS | New patterns for chunk hierarchy navigation, taxonomy evolution, and cross-scheme traversal |

All v1 subsystems, interfaces, and composition patterns remain valid. v2 extends them with hierarchical organization and dynamic evolution capabilities.

---

## 2. Design Goals (Updated)

The architecture pursues the same core goals as v1, with refined emphasis:

1. **Modularity & Composability** — Subsystems are independent at the interface boundary and can be assembled into configurations ranging from minimal context agents to full cognitive architectures. *(v1)*

2. **Separation of Memory from Knowledge** — Memory provides organizational structure across time; knowledge provides content representations of the world. They are distinct axes. *(v1)*

3. **Research-Grounded Design** — Every architectural decision traces to cited academic or cognitive-science research. No design decision is invented without grounding. *(v1)*

4. **[v2] Hierarchical Organization** — Memory and knowledge are not flat stores. They are hierarchically organized at multiple levels, enabling retrieval at appropriate granularity. Episodic memory nests events within schemas; knowledge taxonomies use multi-level hierarchies with direct parent-child relations.

5. **[v2] Dynamic Adaptability** — Taxonomies and knowledge structures evolve as agents encounter new concepts. The architecture includes explicit lifecycle management for taxonomy changes, with provenance tracking and semantic drift detection.

6. **[v2] Multi-Dimensional Retrieval** — Retrieval is not limited to temporal or similarity queries. The architecture supports temporal, hierarchical, semantic, and cue-based (slot-value) retrieval, routed appropriately by the query router.

---

## 3. Architecture Overview (Updated Components)

The architecture is organized into five composable layers. v2 adds hierarchical organization within memory subsystems, a SKOS-aligned taxonomy with an evolution pipeline, and enhanced retrieval with cue-based queries.

```
┌──────────────────────────────────────────────────────────────────┐
│                     ORCHESTRATION LAYER                           │
│              (Agent Loop / Central Executive)                     │
├──────────────────────────────────────────────────────────────────┤
│                      RETRIEVAL LAYER (v2)                         │
│  Query Router · Ranker · Provenance Tracker · Context Composer    │
│  [v2: cue-based retrieval · hierarchical query routing]           │
├───────────────────────────┬──────────────────────────────────────┤
│   MEMORY SUBSYSTEMS (v2)  │      KNOWLEDGE SUBSYSTEMS (v2)        │
│                           │                                       │
│  Working Memory           │  Flat Facts Store                    │
│  Episodic Memory          │  Hierarchical Taxonomy (SKOS-aligned) │
│    [v2: chunk hierarchy]  │    [v2: SKOS + evolution pipeline]   │
│  Semantic Memory          │  Vector Embedding Store (RAG)        │
│    [v2: dual-index]       │  Knowledge Graph                     │
│  Procedural Memory        │    [v2: cross-scheme mapping]        │
│    [v2: skill chunking]   │                                       │
├───────────────────────────┴──────────────────────────────────────┤
│                    STORAGE LAYER (v2)                             │
│  (T1–T5 with hierarchical organization within tiers)             │
└─────────────────────���────────────────────────────────────────────┘
```

**Design rationale**: The separation of memory subsystems from knowledge subsystems follows the academic synthesis that memory provides the *organizational structure* while knowledge provides the *content forms* ([CoALA, arXiv:2309.02427](https://arxiv.org/abs/2309.02427)). The v2 hierarchical extensions follow research showing that "memory is not flat but hierarchically organized at multiple levels" and that "conversational agents benefit from memory indexes that are both temporal and hierarchical" ([Temporal Order Matters, arXiv:2606.04555](https://arxiv.org/html/2606.04555)).

---

## 4. Memory Subsystems (v2 — with Hierarchy)

Memory subsystems manage **agent state across time** — what happened, what is happening now, and how to act. The four-tier taxonomy maps directly to CoALA's modules ([arXiv:2309.02427](https://arxiv.org/abs/2309.02427)), which derive from Tulving's trichotomy and classical cognitive architectures ACT-R and Soar ([arXiv:2201.09305](https://arxiv.org/abs/2201.09305); [Soar, arXiv:2205.03854](https://arxiv.org/pdf/2205.03854)).

**[v2] Hierarchical Memory Principle**: All memory subsystems now support hierarchical organization. Episodic memory nests events within schemas; semantic memory uses dual-indexing (temporal + hierarchical); procedural memory uses skill chunking. This follows ACT-R's chunk-based model, Schema Theory's hierarchical frameworks, and recent AI research on temporal + hierarchical indexing ([ACT-R](https://en.wikipedia.org/wiki/ACT-R); [Schema Theory](https://www.sciencedirect.com/topics/psychology/memory-schema); [arXiv:2606.04555](https://arxiv.org/html/2606.04555)).

### 4.1 Working Memory

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Hold active context for in-process reasoning: current conversation, task state, intermediate results. The only ephemeral tier. |
| **Inputs** | Incoming user messages, tool outputs, retrieved context from other subsystems (injected by the retrieval layer). |
| **Outputs** | Active context window fed to the LLM; eviction signals when capacity is exceeded (triggering archival to episodic memory). |
| **Persistence** | Volatile — lost when session ends. |
| **Capacity** | Bounded by context window size. Maps to Baddeley's limited-capacity working memory model and Miller's 7±2 chunk constraint ([Wikipedia — Baddeley's Model](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory)). |
| **Interface** | `read_context() → ContextWindow` · `write_fragment(fragment)` · `evict(n_fragments) → ArchivedFragments` |

*No v2 changes.* Working memory remains volatile by definition — "the model forgets everything between runs so the memory has to be on disk and not in the context" ([Loop Engineering video](https://youtu.be/GrNbuWWJYiI)). Eviction bridges to episodic memory via OS-style paging ([arXiv:2310.08560](https://arxiv.org/abs/2310.08560)).

### 4.2 Episodic Memory **[v2 — with Chunk Hierarchy]**

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Store timestamped event records — what happened, when, and in what context. Supports "remembering" rather than "knowing." **[v2]** Events are organized into a chunk hierarchy: events → episodes → schemas → domain ontologies. |
| **Inputs** | Archived working memory fragments (eviction), session events (tool calls, user turns, agent actions), explicit store calls. **[v2]** Events are assigned to parent schemas during ingestion. |
| **Outputs** | Retrieved event sequences matching temporal, contextual, or hierarchical queries; raw conversational traces for consolidation. **[v2]** Retrieval can target any level of the chunk hierarchy. |
| **Persistence** | Persistent — time-series store with hierarchical index. |
| **Retrieval mode** | **[v2]** Recall by time, session ID, contextual similarity, **cue (slot-value pair)**, or **hierarchical traversal** (retrieve at appropriate granularity). |
| **Interface** | `record_event(event: Event)` · `recall(query: TimeQuery \| ContextQuery \| CueQuery \| HierarchicalQuery) → Event[]` · `consolidate() → SemanticFacts` **[v2]** · `get_schema(event_id) → Schema` · `traverse_hierarchy(node, direction) → Node[]` |

#### [v2] Chunk Hierarchy Model

Episodic memory now organizes events into a four-level hierarchy, following ACT-R's chunk-based model and Schema Theory's hierarchical organization:

```
Level 4: Domain Ontology   (e.g., "Software Engineering")
   │
Level 3: Schema            (e.g., "Bug Triage Workflow")
   │
Level 2: Episode           (e.g., "Triage session 2024-03-15")
   │
Level 1: Raw Event         (e.g., "User reported login failure")
```

**Grounding**: ACT-R stores chunks as typed objects with slot-value pairs, where "the only way to access a chunk is by specifying a cue, which is a slot-value pair or a set of such pairs" ([ACT-R](https://en.wikipedia.org/wiki/ACT-R); [PSU ACT-R Paper](https://acs.ist.psu.edu/papers/ritterTOip.pdf)). Schema Theory confirms that "schemas possess... hierarchical organization" and are "based on multiple episodes" ([ScienceDirect: Memory Schema](https://www.sciencedirect.com/topics/psychology/memory-schema); [Schema Theory](https://www.cognitivepsychology.com/Schema_Theory)).

#### [v2] Schema Properties (from Schema Theory)

Each schema at Level 3 inherits properties defined by Schema Theory research:

| Property | Description |
|----------|-------------|
| Associative network structure | Links to related schemas and episodes |
| Based on multiple episodes | Generalized from recurring event patterns |
| Lack of unit detail | Abstracts away individual event specifics |
| Adaptability | Modifies based on new experiences |
| Chronological relationships | Temporal ordering within the schema |
| Hierarchical organization | Nesting within domain ontologies |
| Cross-connectivity | Links across schema boundaries |
| Embedded response options | Action templates derived from the schema |

**Source**: Schema Theory's "four necessary features" plus four additional properties ([ScienceDirect](https://www.sciencedirect.com/topics/psychology/memory-schema); [Cognitive Psychology](https://www.cognitivepsychology.com/Schema_Theory)).

#### [v2] Cue-Based Retrieval

Following ACT-R, episodic memory chunks are accessed by **cue** (slot-value pair), not by address or index:

```python
# [v2] Cue-based retrieval interface
CueQuery {
    cues: list[Cue]   # e.g., [{"slot": "event_type", "value": "tool_call"},
                      #        {"slot": "tool", "value": "web_search"}]
    match_mode: "exact" | "partial" | "best_match"
}

# Result: all events matching ALL specified cue slot-value pairs
```

**Grounding**: "Chunks are not indexed in any way and cannot be accessed via their index or their memory address. The only way to access a chunk is by specifying a cue, which is a slot-value pair or a set of such pairs" ([ACT-R](https://en.wikipedia.org/wiki/ACT-R)).

#### [v2] Construction vs. Navigation Separation

Memory construction (how events are structured) is separated from memory navigation (how they are retrieved), following the "Organize then Retrieve" research:

| Module | Responsibility | Interface |
|--------|---------------|-----------|
| **Construction Module** | Iteratively refines how experiences are structured into the chunk hierarchy; distinguishes retrieval failures caused by missing information vs. misleading/overloaded context | `construct(events: Event[]) → Hierarchy` · `refine(failure_signal: RetrievalFailure) → HierarchyUpdate` |
| **Navigation Module** | Retrieves task-relevant context by traversing the hierarchy; selects minimal yet sufficient context at the appropriate granularity level | `navigate(hierarchy: Hierarchy, query: Query) → Context[]` |

**Grounding**: "The construction module iteratively refines how experiences are structured by distinguishing between failures caused by missing information and those caused by misleading or overloaded context. The navigation module retrieves task-relevant context by traversing the hierarchy using a lightweight agent trained with reinforcement learning to select minimal yet sufficient context" ([Organize then Retrieve, arXiv:2606.11680](https://arxiv.org/html/2606.11680v1)).

### 4.3 Semantic Memory **[v2 — with Dual-Index]**

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Store durable, structured facts and knowledge about the world. Supports "knowing" — time-independent information. **[v2]** Now uses dual-index: temporal ordering + hierarchical tree structure. |
| **Inputs** | Consolidated facts from episodic memory (`consolidate()` output), explicit fact injection, knowledge base updates. **[v2]** Facts are assigned to hierarchical concept nodes on ingestion. |
| **Outputs** | Fact lookups, structured knowledge for grounding responses, ontology queries. **[v2]** Can be queried via temporal, hierarchical, or cue-based access. |
| **Persistence** | Persistent — structured database and/or knowledge graph with dual-index. |
| **Retrieval mode** | **[v2]** Direct query, inference over knowledge graph, semantic similarity search, **cue-based retrieval**, or **hierarchical traversal**. |
| **Interface** | `store_fact(fact: Fact)` · `query(predicate) → Fact[]` · `update_fact(fact_id, update)` · `resolve_entity(entity) → EntityNode` **[v2]** · `recall_cue(cues: Cue[]) → Fact[]` · `traverse_concept(concept_id, direction) → Concept[]` |

#### [v2] Dual-Index System

Semantic memory maintains two complementary indexes, following research on temporal + hierarchical memory for conversational agents:

| Index | Purpose | Supports |
|-------|---------|----------|
| **Temporal Index** | Chronological ordering of fact acquisition | Time-based queries, recency ranking, temporal decay |
| **Hierarchical Index** | Tree structure of concept relationships | Granularity selection, ancestor/descendant traversal, schema-level retrieval |

**Grounding**: "The central takeaway is that conversational agents benefit from memory indexes that are both temporal and hierarchical: temporal order helps construct coherent memory states, while the tree structure lets retrieval select context at the appropriate granularity" ([Temporal Order Matters, arXiv:2606.04555](https://arxiv.org/html/2606.04555)).

### 4.4 Procedural Memory **[v2 — with Skill Chunking]**

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Store action instructions — how to perform tasks and operations. Encodes skills, tool-selection logic, behavioral routines. **[v2]** Skills are organized into chunks with hierarchical relationships (atomic actions → composite skills → behavioral patterns). |
| **Inputs** | Learned action patterns (from successful task completions), explicitly authored skill definitions, tool-call schemas. |
| **Outputs** | Applicable action templates / production rules for the current situation. |
| **Persistence** | Persistent — rule store or skill registry with hierarchical organization. |
| **Retrieval mode** | Pattern-match against current state (analogous to ACT-R production-rule firing). |
| **Interface** | `register_skill(skill: Skill)` · `match_action(state: AgentState) → ActionTemplate[]` · `update_skill(skill_id, update)` **[v2]** · `decompose_skill(skill_id) → ActionChunk[]` |

#### [v2] Skill Chunk Hierarchy

```
Level 3: Behavioral Pattern   (e.g., "Debug → Fix → Test → Verify")
   │
Level 2: Composite Skill      (e.g., "Fix failing test")
   │
Level 1: Atomic Action        (e.g., "Read stack trace")
```

**Grounding**: ACT-R's declarative/procedural split separates "tool-selection logic (procedural) from knowledge retrieval (semantic)" and organizes production rules into modular chunks ([arXiv:2201.09305](https://arxiv.org/abs/2201.09305); [Soar, arXiv:2205.03854](https://arxiv.org/pdf/2205.03854)).

### Memory Subsystem Interaction Map **[v2]**

```
                 ┌──────────────┐
  user input ──▶ │   Working    │◀── context injection
                 │   Memory     │      (from retrieval layer)
                 └──────┬───────┘
                   evict│
                 ┌──────▼───────┐
                 │  Episodic    │──consolidate()──┐
                 │  Memory      │                 │
                 │  [v2: chunk  │          ┌──────▼───────┐
                 │   hierarchy] │          │  Semantic    │
                 │  ┌────────┐  │          │  Memory      │
                 │  │ events │  │          │  [v2: dual-  │
                 │  │   ↓    │  │          │   index]     │
                 │  │episodes│  │          └──────────────┘
                 │  │   ↓    │  │
                 │  │schemas │  │
                 │  │   ↓    │  │
                 │  │domains │  │
                 │  └────────┘  │
                 └──────────────┘
                 ┌──────────────┐
  task success ─▶│ Procedural   │
                 │ Memory       │
                 │ [v2: skill   │
                 │  chunks]     │
                 └──────────────┘
```

---

## 5. Knowledge Subsystems (v2 — with SKOS and Evolution)

Knowledge subsystems manage **how information about the world is represented and retrieved**. They are *content stores* that the memory subsystems (particularly semantic memory) draw upon. The four representation models span a spectrum from unstructured to highly structured ([academic-research-findings.md §2.1](./academic-research-findings.md)).

### 5.1 Flat Facts Store

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Store raw text snippets, key-value pairs, and simple facts with no structural relationships. |
| **Inputs** | Fact strings, key-value entries, raw document excerpts. |
| **Outputs** | Exact-match or substring-match facts. |
| **Strength** | Simple, universal, low overhead ([DEV Community](https://dev.to/bobur/agent-knowledge-vs-memories-understanding-the-difference-4pgj)). |
| **Weakness** | No relationships, no inference capability. |
| **Interface** | `put(key, value)` · `get(key) → Value` · `search(substring) → Fact[]` |

*No v2 changes.*

### 5.2 Hierarchical Taxonomy **[v2 — SKOS-Aligned with Evolution Pipeline]**

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Organize knowledge into hierarchical concept schemes. **[v2]** Now aligned with the W3C SKOS standard: URI-based concepts, broader/narrower relations, collections, cross-scheme mapping. Includes a dynamic evolution pipeline for taxonomy lifecycle management. |
| **Inputs** | Concept definitions, parent-child relationships, category assignments. **[v2]** Concept proposals from the evolution pipeline, cross-scheme mappings, drift signals. |
| **Outputs** | Category hierarchies, ancestor/descendant lookups, taxonomy traversal. **[v2]** SKOS concept schemes, cross-scheme mappings, facet collections, evolution provenance. |
| **Strength** | Structured categorization; efficient navigation. **[v2]** Standards-aligned (SKOS); dynamically evolvable; facet-capable. |
| **Weakness** | **[v2 reduced]** SKOS broader/narrower restricts to direct parent-child trees, requiring cross-scheme mapping for non-hierarchical relationships. |
| **Interface** | `add_concept(concept, parent)` · `get_ancestors(concept) → Concept[]` · `get_children(concept) → Concept[]` **[v2]** · `add_collection(concepts, label) → Collection` · `map_concept(source_concept, target_scheme, target_concept) → Mapping` · `evolve(proposal: Proposal) → EvolutionResult` |

#### [v2] SKOS-Aligned Concept Schema

The taxonomy subsystem uses the W3C SKOS (Simple Knowledge Organization System) standard as its canonical schema. SKOS provides a formal vocabulary for representing concept schemes with hierarchical and associative relationships.

**SKOS Core Properties**:

| SKOS Property | Architecture Use | Description |
|---------------|-----------------|-------------|
| `skos:Concept` | Base concept type | Each knowledge concept is a URI-identified resource |
| `skos:ConceptScheme` | Domain taxonomy | Groups related concepts into a single controlled vocabulary |
| `skos:broader` | Parent link | Asserts a **direct** (immediate) hierarchical link to a broader concept |
| `skos:narrower` | Child link | Asserts a **direct** (immediate) hierarchical link to a narrower concept |
| `skos:related` | Cross-link | Asserts a non-hierarchical associative relationship |
| `skos:Collection` | Facet grouping | Groups concepts for non-hierarchical purposes (facets, topics) |
| `skos:exactMatch` | Cross-scheme mapping | Maps a concept to an equivalent concept in another scheme |
| `skos:closeMatch` | Approximate mapping | Maps to a related but not identical concept in another scheme |

**Grounding**: "The properties skos:broader and skos:narrower are used to assert a direct hierarchical link between two SKOS concepts. By convention, skos:broader is only used to assert an immediate (i.e. direct) hierarchical link" ([W3C SKOS Reference](https://www.w3.org/TR/skos-reference/)). "SKOS concepts can be identified using URIs, labeled with lexical strings in multiple languages, assigned notations, grouped into concept schemes, grouped into labeled and/or ordered collections, and mapped to concepts in other schemes" ([SKOS Primer](https://www.w3.org/TR/skos-primer/); [ISKO Encyclopedia](https://www.isko.org/cyclo/skos.htm)).

**Key Design Constraint — Direct Parent-Child Only**: Following SKOS convention, broader/narrower relations assert only **immediate** hierarchical links. Transitive closure (ancestor/descendant) is computed at query time, not stored. This maintains tree integrity and avoids multi-parent complexity:

```
                          (only direct links stored)
  Vehicle ──broader──▶ Car ──broader──▶ Sedan
      │                                          
      └─ (ancestor of Sedan computed via traversal, not stored)
```

#### [v2] Collections for Non-Hierarchical Groupings (Facets)

SKOS collections enable grouping concepts that are not hierarchically related — useful for facets, topics, and cross-cutting concerns:

```
Concept Scheme: "Programming Languages"
  ├── Hierarchy: Language → Compiled → C++
  ├── Hierarchy: Language → Interpreted → Python
  └── Collection: "Web Technologies" = {JavaScript, Python, Ruby}  (non-hierarchical facet)
```

**Grounding**: SKOS concepts can be "grouped into labeled and/or ordered collections" for non-hierarchical purposes ([SKOS Primer](https://www.w3.org/TR/skos-primer/); [Hedden Information](https://www.hedden-information.com/skos-taxonomies/)).

#### [v2] Cross-Scheme Mapping Support

Concepts in one scheme can be mapped to concepts in other schemes, enabling knowledge interoperability:

| Mapping Type | Use Case |
|-------------|----------|
| `skos:exactMatch` | Same concept in different schemes (e.g., "DB" in one scheme = "Database" in another) |
| `skos:closeMatch` | Related but not identical (e.g., "Bug" in QA scheme ≈ "Defect" in engineering scheme) |
| `skos:broadMatch` | Target is broader (e.g., "PostgreSQL" → "Database") |
| `skos:narrowMatch` | Target is narrower |

**Grounding**: SKOS supports mapping concepts "to concepts in other schemes" via exact/close/broad/narrow match properties ([W3C SKOS Reference](https://www.w3.org/TR/skos-reference/)).

#### [v2] Dynamic Taxonomy Evolution Pipeline

The taxonomy subsystem includes a four-phase evolution pipeline for managing the lifecycle of new concepts and structural changes:

```
┌──────────┐    ┌──────────┐    ┌────────────┐    ┌──────────┐
│Discovery │───▶│Proposal  │───▶│Validation  │───▶│ Merge    │
│          │    │          │    │            │    │          │
│Detect new│    │Generate  │    │Domain      │    │Integrate │
│concepts  │    │extension │    │experts     │    │with      │
│outside   │    │from      │    │review &    │    │provenance│
│ontology  │    │recurring │    │approve     │    │tracking  │
│          │    │patterns  │    │            │    │          │
└──────────┘    └──────────┘    └────────────┘    └──────────┘
```

| Phase | Responsibility | Interface |
|-------|---------------|-----------|
| **Discovery** | Detect concepts encountered by the agent that fall outside the current taxonomy. Triggers on unrecognized recurring concepts. | `detect_unrecognized(concepts: Concept[]) → Concept[]` |
| **Proposal** | Generate taxonomy extension proposals from recurring unrecognized concepts. Groups terms, hierarchizes, and relates them. | `propose_extensions(concepts: Concept[]) → Proposal[]` |
| **Validation** | Domain experts (or validation rules) review and approve/reject proposals. Checks for conflicts, redundancy, and consistency. | `validate(proposal: Proposal) → ValidationResult` |
| **Merge** | Approved extensions are integrated into the taxonomy with full provenance tracking. | `merge(proposal: Proposal) → MergeResult` |

**Grounding**: "Ontology evolution involves: Discovery — agents encounter concepts outside current ontology; Proposal — system proposes extensions from recurring unrecognized concepts; Validation — domain experts review and approve; Merging — approved extensions integrated with provenance tracking" ([Ontology-Constrained Neural Reasoning, arXiv:2604.00555](https://arxiv.org/html/2604.00555v5)).

#### [v2] Concept Formation Pipeline

New concepts are formed through a four-step pipeline within the Proposal phase:

```
Extract ──▶ Group ──▶ Hierarchize ──▶ Relate
```

| Step | Action |
|------|--------|
| **Extract** | Identify terms and their synonyms from agent interactions |
| **Group** | Organize related terms into meaningful concepts or classes based on similarities |
| **Hierarchize** | Establish taxonomic 'is-a' relationships (e.g., 'car' is-a 'vehicle') |
| **Relate** | Define non-taxonomic relationships (semantic analysis of nouns, verbs) |

**Grounding**: "Concept formation: Once terms and their synonyms are extracted, the next step is to group them into meaningful concepts or classes. Taxonomic relations establish hierarchical relationships between concepts, defining the 'is-a' relationship" ([Ontology Learning Survey, arXiv:2404.14991](https://arxiv.org/html/2404.14991v2)). Non-taxonomic relations are identified via "semantic analysis [that] identifies main components (nouns, verbs) to label non-taxonomic relations" ([MDPI Applied Sciences](https://www.mdpi.com/2076-3417/11/22/10770)).

#### [v2] Semantic Drift Detection

The taxonomy subsystem continuously monitors for semantic drift — changes in concept meaning over time. Drift is detected at two levels:

| Drift Level | What is Monitored | Detection Method |
|-------------|-------------------|-----------------|
| **Concept Level** | Features of individual concepts: intention, extension, labels, URIs | Compare concept features across time windows; flag when features shift beyond threshold |
| **Structural Level** | Taxonomic relations among concepts | Compare tree structure across time; flag when parent-child relationships change |

**Grounding**: "The semantic drift is evaluated at the concept level, by considering the main features involved in an ontology concept (e.g., intention, extension, labels, URIs, etc.) and at the structural level, by inspecting the taxonomic relations among concepts" ([OntoDrift, CEUR-WS Vol-2821](https://ceur-ws.org/Vol-2821/paper1.pdf)).

#### [v2] Provenance Tracking for Taxonomy Changes

Every taxonomy modification — concept addition, relationship change, structural reorganization — is tracked with full provenance:

```python
# [v2] Provenance record for taxonomy changes
ProvenanceRecord {
    change_id: str
    change_type: "add_concept" | "add_relation" | "restructure" | "merge"
    source: "evolution_pipeline" | "explicit" | "consolidation"
    timestamp: datetime
    proposal_id: str          # Links to the evolution proposal that triggered this change
    agent_id: str             # Which agent triggered the change
    validation_status: str    # "approved" | "auto-merged" | "pending"
}
```

**Grounding**: "Approved extensions integrated with provenance tracking" ([arXiv:2604.00555](https://arxiv.org/html/2604.00555v5)). "KGs are generated at runtime and the data is current. Evolution is very likely in KGs" ([arXiv:2201.05910](https://arxiv.org/pdf/2201.05910)).

### 5.3 Vector Embedding Store (RAG)

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Store dense numerical embeddings of text for semantic similarity search. Powers Retrieval-Augmented Generation. |
| **Inputs** | Text chunks → embedding model → vectors; query text → embedding → similarity search. |
| **Outputs** | Top-k semantically similar text chunks for context injection. |
| **Strength** | Semantic similarity search across unstructured text; handles noisy, natural-language queries. |
| **Weakness** | Loses explicit relationships; cannot reason over multi-hop connections ([GoodData.AI](https://www.gooddata.ai/blog/from-rag-to-graphrag-knowledge-graphs-ontologies-and-smarter-ai/)). |
| **Interface** | `embed_and_store(text, metadata)` · `similarity_search(query_text, k) → Chunk[]` |

*No v2 changes.* RAG retrieves raw text, not structured knowledge — motivating the knowledge graph subsystem ([academic-research-findings.md §2.2](./academic-research-findings.md)).

### 5.4 Knowledge Graph **[v2 — with Cross-Scheme Mapping]**

| Aspect | Definition |
|--------|-----------|
| **Responsibility** | Store typed entities connected by typed relationship edges, governed by an ontology schema. Supports inference and multi-hop reasoning. **[v2]** Now supports cross-scheme mapping — entities can reference concepts in the SKOS taxonomy, enabling graph-to-taxonomy traversal. |
| **Inputs** | Entity definitions, typed relationships, ontology schemas. **[v2]** Cross-scheme references to SKOS concepts. |
| **Outputs** | Multi-hop query results, inferred facts, relationship traversals, entity subgraphs. **[v2]** Cross-references to hierarchical taxonomy concepts. |
| **Strength** | Inference, multi-hop reasoning, explainability ([zbrain.ai](https://zbrain.ai/knowledge-graphs-for-agentic-ai/)). **[v2]** Can navigate between graph entities and taxonomy concepts. |
| **Weakness** | Maintenance overhead; requires ontology engineering. |
| **Interface** | `add_entity(entity: EntityNode)` · `add_relationship(subject, predicate, object)` · `traverse(start, path_pattern) → Subgraph` · `infer(query) → InferredFact[]` **[v2]** · `link_to_concept(entity_id, concept_uri) → Link` · `traverse_cross_scheme(entity_id) → Concept[]` |

**[v2] Cross-Scheme Integration**: The knowledge graph can reference SKOS taxonomy concepts via URI, enabling agents to navigate from a graph entity to its position in the hierarchical taxonomy, and vice versa. This combines relational reasoning (graph) with hierarchical categorization (taxonomy).

**Hybrid composition — GraphRAG**: The architecture supports composing the vector store (§5.3) and knowledge graph (§5.4) into a **GraphRAG** pattern — graph-structured knowledge representation combined with graph-based retrieval for context-preserving, multi-hop reasoning ([arXiv:2501.13958](https://arxiv.org/html/2501.13958v1)).

---

## 6. Retrieval Layer (v2 — with Cue-Based Retrieval)

The retrieval layer sits between the memory/knowledge subsystems and the orchestration layer. It is the **integration point** — analogous to Baddeley's episodic buffer, which "integrates information from multiple sources" and "binds information into coherent episodes" ([Wikipedia — Baddeley's Model](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory)).

**[v2] Multi-Dimensional Retrieval**: The retrieval layer now supports four retrieval dimensions: temporal (when), hierarchical (what level of granularity), semantic (what meaning), and cue-based (what attributes). The query router selects the appropriate dimension(s) based on the query type.

### 6.1 Components (v2)

| Component | Responsibility | Interface |
|-----------|---------------|-----------|
| **Query Router** | Determine which subsystem(s) to query and which retrieval mode to use. **[v2]** Now routes hierarchical queries (traverse a hierarchy at a given granularity), cue-based queries (match slot-value pairs), and cross-scheme queries. | `route(query: Query) → SubsystemTarget[]` **[v2]** · `route_hierarchical(query: HierarchicalQuery) → SubsystemTarget[]` · `route_cue(query: CueQuery) → SubsystemTarget[]` |
| **Ranker** | Score and order retrieved results by relevance, recency, and confidence. Merge results from multiple subsystems into a unified ranking. **[v2]** Supports cue-based scoring — results matching more cue slot-value pairs rank higher. | `rank(results: Result[], query) → RankedResults` |
| **Provenance Tracker** | Attach source attribution to every retrieved item: which subsystem, what source, when stored, retrieval confidence. **[v2]** Also tracks taxonomy evolution provenance — which version of a concept was used. | `annotate(result: Result) → ResultWithProvenance` |
| **Context Composer** | Assemble the final context payload to inject into working memory, respecting capacity constraints. | `compose(ranked: RankedResults, budget: int) → ContextPayload` |

### 6.2 [v2] Retrieval Modes

The retrieval layer supports four retrieval modes, each routed to the appropriate subsystem(s):

| Mode | Query Type | Target Subsystem(s) | Use Case |
|------|-----------|---------------------|----------|
| **Temporal** | `TimeQuery` | Episodic Memory | "What happened during session X?" |
| **Hierarchical** | `HierarchicalQuery` | Episodic, Semantic, Taxonomy | "Give me the schema for these events" / "Retrieve at domain level" |
| **Semantic** | `ContextQuery` / `SimilarityQuery` | Vector Store, Semantic Memory | "Find similar past experiences" |
| **Cue-Based** | `CueQuery` | Episodic, Semantic, Procedural | "Find all events where tool=X and outcome=success" |

**Query Router Logic**:

```
Incoming Query
      │
      ▼
┌──────────────┐
│ Query Router │
└──────┬───────┘
       │
  ┌────┼────────────┬────────────────┬─────────────────┐
  ▼    ▼            ▼                ▼                 ▼
TimeQuery     CueQuery      HierarchicalQuery   SimilarityQuery
  │            │                │                   │
  ▼            ▼                ▼                   ▼
Episodic   Episodic/Sem/    Episodic/Sem/       Vector Store /
Memory     Proc Memory      Taxonomy            Knowledge Graph
```

### 6.3 Retrieval Data Flow (v2)

```
User Query / Agent State / Cue
        │
        ▼
┌───────────────┐
│ Query Router  │──── determines target subsystems + retrieval mode
└───────┬───────┘     [v2: temporal / hierarchical / semantic / cue-based]
        │ parallel fan-out
   ┌────┼────┬────────┬─────────┬──────────┐
   ▼    ▼    ▼        ▼         ▼          ▼
 Work  Epi  Sem     Vector    Graph     Taxonomy
 Mem   Mem  Mem     Store     Store     [v2: SKOS]
   │    │    │        │         │          │
   └────┴────┴────────┴─────────┴──────────┘
        │
        ▼
┌───────────────┐
│   Ranker      │──── scores by relevance + recency + confidence
│               │     [v2: + cue match score + hierarchical granularity]
└───────┬───────┘
        ▼
┌───────────────┐
│ Provenance    │──── attaches source attribution
│ Tracker       │     [v2: + taxonomy version / concept provenance]
└───────┬───────┘
        ▼
┌───────────────┐
│ Context       │──── assembles within capacity budget
│ Composer      │
└───────┬───────┘
        ▼
  Context Payload ──▶ Working Memory
```

### 6.4 Predictive Retrieval (Proactive Loading)

Beyond reactive query-driven retrieval, the architecture supports **predictive retrieval** inspired by Friston's predictive processing framework. The agent maintains expectation models about what context is likely relevant; when predictions are violated (prediction error), this acts as a surprise signal that triggers proactive context loading ([Predictably Correct Substack](https://predictablycorrect.substack.com/p/a-predictive-processing-perspective)).

| Predictive Processing Concept | Architecture Implementation |
|------------------------------|---------------------------|
| Generative models | Prior expectation models over likely-relevant context |
| Prediction errors | Surprise signals triggering context retrieval |
| Active inference | Proactive context loading before explicit queries |
| Hierarchical predictions | Multi-level memory hierarchy navigation (episodic → semantic → procedural) |

**Interface**: `predict_context(state: AgentState) → RetrievalHints` — generates retrieval hints proactively, which the query router can consume alongside explicit queries.

*No v2 changes.* Predictive retrieval operates as before, but its hierarchical prediction outputs can now feed into the v2 hierarchical query router.

---

## 7. Storage Model (T1–T5 with Hierarchy Notes)

The storage layer maps subsystems to concrete persistence technologies. No single storage engine serves all subsystems; the architecture specifies multiple storage tiers, consistent with MemGPT's tiered virtual memory model ([arXiv:2310.08560](https://arxiv.org/abs/2310.08560)).

| Storage Tier | Volatility | Latency | Serves Subsystem(s) | Technology Examples | **[v2] Hierarchy Notes** |
|-------------|-----------|---------|----------------------|---------------------|--------------------------|
| **T1 — Context/Working** | Volatile | Fastest | Working Memory | In-process context window, in-memory state | Flat — no hierarchy (volatile) |
| **T2 — Structured DB** | Persistent | Moderate | Episodic, Semantic, Flat Facts, **[v2]** Taxonomy | PostgreSQL, SQLite, document stores | **[v2]** Stores chunk hierarchy, dual-index, SKOS concept schemes, evolution provenance |
| **T3 — Vector Store** | Persistent | Moderate | Vector Embeddings (RAG), Semantic (similarity) | Pinecone, Weaviate, Chroma | **[v2]** Can store hierarchical metadata for granularity-filtered similarity search |
| **T4 — Graph Store** | Persistent | Moderate | Knowledge Graph, **[v2]** Taxonomy (SKOS broader/narrower as edges) | Neo4j, property graphs | **[v2]** SKOS broader/narrower stored as typed edges; cross-scheme mappings as typed edges |
| **T5 — Cold Archive** | Persistent | High | Episodic (raw logs), Procedural (historical), **[v2]** Taxonomy (historical versions) | Object storage, log archives | **[v2]** Archives previous taxonomy versions for provenance history |

**Design decision**: The multi-tier model follows MemGPT's insight that "OS-style paging between context and external storage" is necessary because working memory is bounded but agents accumulate state indefinitely ([arXiv:2310.08560](https://arxiv.org/abs/2310.08560)). Letta's three-tier model (core / archival / recall) and Zep's temporal knowledge graphs are production exemplars ([arXiv:2606.24775](https://arxiv.org/html/2606.24775v1)).

**[v2] Hierarchical Organization Within Tiers**: T2 (structured DB) stores the chunk hierarchy and dual-index for episodic/semantic memory. T4 (graph store) is ideal for SKOS broader/narrower relationships — each relation is a typed edge, enabling efficient ancestor/descendant traversal via graph queries. T5 archives historical taxonomy versions, enabling provenance queries over time.

---

## 8. Composition Patterns (v2 — with New Hierarchy Patterns)

The architecture's value lies in its composability. Subsystems are independent modules that assemble into coherent agent configurations. v2 adds patterns for hierarchical memory navigation and taxonomy evolution.

### Pattern A: Minimal Context Agent (Working + Episodic)

```
Working Memory ⇄ Episodic Memory
```

The simplest composition: an agent that maintains active context and logs events for session continuity but has no long-term knowledge. *(v1, unchanged)*

### Pattern B: Knowledge-Grounded Agent (Working + Semantic + Vector Store)

```
Working Memory ⇄ Semantic Memory ⇄ Vector Embedding Store (RAG)
```

Standard RAG agent pattern. *(v1, unchanged)*

### Pattern C: Skill-Augmented Agent (Pattern B + Procedural)

```
Working Memory ⇄ Semantic Memory ⇄ Vector Store
                 Procedural Memory (skill registry)
```

Adds procedural memory for tool-use and task execution. *(v1, unchanged)*

### Pattern D: Full Cognitive Architecture (All Subsystems)

```
         ┌──────────────────────────────────┐
         │         Working Memory            │
         └──────┬──────────┬─────────────────┘
                │          │
    ┌───────────▼──┐  ┌───▼────────────┐
    │ Episodic     │  │ Semantic       │
    │ Memory       │  │ Memory         │
    │ [v2: chunk   │  │ [v2: dual-     │
    │  hierarchy]  │  │  index]        │
    └──────┬───────┘  └───┬────────┬───┘
           │              │        │
    (consolidate)    Vector    Knowledge
                     Store     Graph
                │              │
    ┌───────────▼──────────┐   │
    │ Procedural Memory    │   │
    │ [v2: skill chunks]   │   │
    └──────────────────────┘   │
                               │
                    ┌──────────▼──────────┐
                    │ Hierarchical        │
                    │ Taxonomy [v2: SKOS] │
                    │ + Evolution Pipeline│
                    └─────────────────────┘
```

All memory and knowledge subsystems integrated via the retrieval layer. *(v1 extended with v2 hierarchical capabilities)*

### Pattern E: GraphRAG Composition (Vector Store + Knowledge Graph)

```
Vector Embedding Store ──┐
                         ├──▶ Unified Retrieval (GraphRAG)
Knowledge Graph ─────────┘
```

Neural-symbolic hybrid approach. *(v1, unchanged)*

### Pattern F: Consolidation Pipeline (Episodic → Semantic)

```
Episodic Memory ──consolidate()──▶ Semantic Memory ──▶ Knowledge Graph
```

Temporal composition mirroring hippocampal-neocortical consolidation. *(v1, unchanged)*

### **[v2] Pattern G: Hierarchical Memory Navigation (Construction + Navigation)**

```
Event Stream ──▶ Construction Module ──▶ Chunk Hierarchy ──▶ Navigation Module ──▶ Context
                 (structure events)       (events→episodes      (traverse at        (minimal,
                                          →schemas→domains)     appropriate          sufficient)
                                          + cue indexing         granularity)
```

This pattern uses the construction/navigation separation from the "Organize then Retrieve" research. Events are structured into a chunk hierarchy by the construction module, then retrieved at the appropriate granularity by the navigation module. The construction module iteratively refines based on retrieval failures — distinguishing missing information from misleading/overloaded context.

**Grounding**: [arXiv:2606.11680](https://arxiv.org/html/2606.11680v1) — "The construction module iteratively refines how experiences are structured... The navigation module retrieves task-relevant context by traversing the hierarchy."

### **[v2] Pattern H: Taxonomy Evolution Pipeline (Discovery → Proposal → Validation → Merge)**

```
Agent Interactions
       │
       ▼
┌──────────┐    ┌──────────────┐    ┌────────────┐    ┌──────────┐
│Discovery │───▶│ Proposal     │───▶│ Validation │───▶│ Merge    │
│          │    │ (Extract→    │    │            │    │ (+ Prov. │
│          │    │  Group→      │    │            │    │  Track)  │
│          │    │  Hierarchize→│    │            │    │          │
│          │    │  Relate)     │    │            │    │          │
└──────────┘    └──────────────┘    └────────────┘    └────┬─────┘
                                                        │
                                                        ▼
                                              Updated Taxonomy
                                              (SKOS-aligned)
```

This pattern manages the lifecycle of new concepts entering the taxonomy. Unrecognized concepts are discovered, proposals are generated through the concept formation pipeline, validated, and merged with provenance tracking. Semantic drift detection runs continuously alongside this pipeline.

**Grounding**: [arXiv:2604.00555](https://arxiv.org/html/2604.00555v5) — ontology evolution phases; [arXiv:2404.14991](https://arxiv.org/html/2404.14991v2) — concept formation; [OntoDrift](https://ceur-ws.org/Vol-2821/paper1.pdf) — drift detection.

### **[v2] Pattern I: Cross-Scheme Traversal (Taxonomy + Knowledge Graph)**

```
Hierarchical Taxonomy (SKOS) ──cross-scheme mapping──▶ Knowledge Graph
                                                           │
                    ┌──────────────────────────────────────┘
                    │
                    ▼
            Entity ↔ Concept Navigation
            (graph entity references taxonomy concept via URI)
```

This pattern enables agents to navigate between relational reasoning (knowledge graph) and hierarchical categorization (SKOS taxonomy). An entity in the graph can reference a concept in the taxonomy, and the agent can traverse from the entity to its hierarchical context, or from a concept to its related entities.

**Grounding**: [W3C SKOS Reference](https://www.w3.org/TR/skos-reference/) — cross-scheme mapping; [arXiv:2501.13958](https://arxiv.org/html/2501.13958v1) — graph + knowledge representation integration.

---

## 9. Data Flow: End-to-End Agent Cycle (v2 — with Evolution Pipeline)

The following traces a single agent cycle through the v2 architecture:

1. **Perception**: A user message or environment signal enters **Working Memory** via the orchestration layer's agent loop.

2. **Prediction** *(optional)*: The **predictive retrieval** component generates expectation-based retrieval hints from the current agent state. Prediction errors generate surprise signals ([Predictably Correct](https://predictablycorrect.substack.com/p/a-predictive-processing-perspective)).

3. **Retrieval**: The **Query Router** determines which subsystems to query and which **retrieval mode** to use:
   - Temporal queries → **Episodic Memory** (`recall()` with `TimeQuery`)
   - **[v2]** Cue-based queries → **Episodic/Semantic/Procedural Memory** (`recall()` with `CueQuery`) — matching slot-value pairs
   - **[v2]** Hierarchical queries → **Episodic/Semantic/Taxonomy** (`traverse_hierarchy()`) — retrieving at appropriate granularity
   - Factual queries → **Semantic Memory** (`query()`) → **Vector Store** and/or **Knowledge Graph**
   - Action/skill queries → **Procedural Memory** (`match_action()`)

4. **Ranking & Provenance**: The **Ranker** scores results by relevance, recency, confidence, **[v2] cue match score, and hierarchical granularity**. The **Provenance Tracker** attaches source attribution — tracking *how* information was retrieved (direct experience vs. learned knowledge) and **[v2] which taxonomy version was used**.

5. **Context Composition**: The **Context Composer** assembles the final payload within working memory's capacity budget.

6. **Reasoning & Action**: The orchestration layer processes the composed context and selects an action ([CoALA, arXiv:2309.02427](https://arxiv.org/abs/2309.02427)).

7. **Recording**: The action and its outcome are recorded as an event in **Episodic Memory** (`record_event()`). **[v2]** The event is assigned to its parent schema in the chunk hierarchy.

8. **Consolidation** *(periodic)*: Accumulated episodic events are consolidated into **Semantic Memory** facts via `consolidate()`, which may then be structured into the **Knowledge Graph** or **[v2]** the **Taxonomy**.

9. **Skill Learning** *(on task success)*: Successful action patterns are registered in **Procedural Memory** (`register_skill()`).

10. **[v2] Taxonomy Evolution** *(as-needed)*: If the agent encounters concepts outside the current taxonomy:
    - **Discovery**: Unrecognized concepts are detected
    - **Proposal**: Concept formation pipeline (Extract → Group → Hierarchize → Relate) generates extension proposals
    - **Validation**: Proposals are reviewed for consistency and conflicts
    - **Merge**: Approved extensions are integrated with provenance tracking
    - **Drift Check**: Semantic drift detection monitors existing concepts at the concept and structural levels

---

## 10. Design Decisions (v1: D1–D18 + v2: D19–D28)

Every architectural decision traces to cited research. v1 decisions D1–D18 remain valid. v2 adds D19–D28.

### v1 Design Decisions (Retained)

| # | Design Decision | Source | Citation |
|---|----------------|--------|----------|
| D1 | Four-tier memory taxonomy (working, episodic, semantic, procedural) | CoALA framework | [arXiv:2309.02427](https://arxiv.org/abs/2309.02427) |
| D2 | Working memory is volatile; persistence requires external storage | Loop Engineering video | [YouTube](https://youtu.be/GrNbuWWJYiI) |
| D3 | Eviction bridges working → episodic memory (OS-style paging) | MemGPT | [arXiv:2310.08560](https://arxiv.org/abs/2310.08560) |
| D4 | Episodic records preserve full binding context (time, source, task) | Schema theory / context binding | [Frontiers](https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2023.1217093/full) |
| D5 | Consolidation pipeline: episodic → semantic → knowledge graph | Hippocampal-neocortical consolidation | [Frontiers](https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2023.1217093/full); [arXiv:2605.17625](https://arxiv.org/html/2605.17625v1) |
| D6 | Procedural memory separated from semantic (declarative/procedural split) | ACT-R; Soar | [arXiv:2201.09305](https://arxiv.org/abs/2201.09305); [arXiv:2205.03854](https://arxiv.org/pdf/2205.03854) |
| D7 | Four knowledge representation models (flat, hierarchical, vector, graph) | Knowledge representation spectrum | [academic-research-findings.md §2.1](./academic-research-findings.md) |
| D8 | RAG retrieves text, not structured knowledge — motivating knowledge graphs | RAG limitation analysis | [GoodData.AI](https://www.gooddata.ai/blog/from-rag-to-graphrag-knowledge-graphs-ontologies-and-smarter-ai/) |
| D9 | Knowledge graphs use ontologies for inference of non-explicit facts | Knowledge graph / ontology literature | [Enterprise Knowledge](https://enterprise-knowledge.com/ontology-and-knowledge-graph-in-the-age-of-ai-and-agents/) |
| D10 | GraphRAG composes vector + graph for multi-hop reasoning | GraphRAG survey | [arXiv:2501.13958](https://arxiv.org/html/2501.13958v1) |
| D11 | Retrieval layer as integration point (Baddeley's episodic buffer analog) | Baddeley's working memory model | [Wikipedia](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory) |
| D12 | Provenance tracking (remember/know distinction) | Tulving's R/K distinction | [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S1053810009000804) |
| D13 | Predictive retrieval via expectation models and prediction-error signals | Friston's free energy principle | [Predictably Correct](https://predictablycorrect.substack.com/p/a-predictive-processing-perspective) |
| D14 | Multi-tier storage model (volatile → DB → vector → graph → archive) | MemGPT tiered memory; Letta | [arXiv:2310.08560](https://arxiv.org/abs/2310.08560); [arXiv:2606.24775](https://arxiv.org/html/2606.24775v1) |
| D15 | Agent loop separates internal actions (reason) from external actions (tools) | CoALA action space | [arXiv:2309.02427](https://arxiv.org/abs/2309.02427) |
| D16 | Neural-symbolic hybrid: graph scaffold + embedding interface | Symbolic/subsymbolic integration | [Neo4j blog](https://neo4j.com/blog/developer/knowledge-graph-structured-semantic-search/) |
| D17 | Episodic buffer integration binds multi-source context | Baddeley's episodic buffer (2000) | [Wikipedia](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory) |
| D18 | Working memory bounded capacity (Miller's 7±2 analog) | Baddeley / Miller | [Wikipedia](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory) |

### [v2] New Design Decisions

| # | Design Decision | Source | Citation |
|---|----------------|--------|----------|
| D19 | Episodic memory uses chunk hierarchy: events → episodes → schemas → domain ontologies | ACT-R chunk model; Schema Theory | [ACT-R](https://en.wikipedia.org/wiki/ACT-R); [ScienceDirect: Memory Schema](https://www.sciencedirect.com/topics/psychology/memory-schema) |
| D20 | Retrieval is cue-based (slot-value pair matching), not address-based | ACT-R declarative memory | [ACT-R](https://en.wikipedia.org/wiki/ACT-R); [PSU Paper](https://acs.ist.psu.edu/papers/ritterTOip.pdf) |
| D21 | Semantic memory uses dual-index: temporal ordering + hierarchical tree structure | Temporal + hierarchical indexing research | [arXiv:2606.04555](https://arxiv.org/html/2606.04555) |
| D22 | Memory construction (structuring) is separated from memory navigation (retrieval) | Organize then Retrieve | [arXiv:2606.11680](https://arxiv.org/html/2606.11680v1) |
| D23 | Schemas possess 8 properties: associative network, multi-episode basis, abstraction, adaptability, chronological ordering, hierarchical organization, cross-connectivity, embedded responses | Schema Theory | [ScienceDirect](https://www.sciencedirect.com/topics/psychology/memory-schema); [Cognitive Psychology](https://www.cognitivepsychology.com/Schema_Theory) |
| D24 | Taxonomy subsystem aligned with W3C SKOS: URI concepts, broader/narrower (direct only), collections, cross-scheme mapping | W3C SKOS standard | [W3C SKOS Reference](https://www.w3.org/TR/skos-reference/); [SKOS Primer](https://www.w3.org/TR/skos-primer/) |
| D25 | Broader/narrower relations assert only immediate parent-child links (transitive closure computed at query time) | SKOS convention | [W3C SKOS Reference](https://www.w3.org/TR/skos-reference/) |
| D26 | Taxonomy evolution follows 4-phase pipeline: Discovery → Proposal → Validation → Merge | Ontology evolution research | [arXiv:2604.00555](https://arxiv.org/html/2604.00555v5) |
| D27 | Concept formation pipeline: Extract → Group → Hierarchize → Relate | Ontology learning research | [arXiv:2404.14991](https://arxiv.org/html/2404.14991v2) |
| D28 | Semantic drift detection at concept level (features) + structural level (taxonomic relations) | OntoDrift | [CEUR-WS Vol-2821](https://ceur-ws.org/Vol-2821/paper1.pdf) |

---

## 11. Gaps and Additional Research Needed

The following areas require further investigation:

1. **Consolidation trigger policies**: *When* should episodic consolidation run? *(v1, retained)*

2. **Cross-agent / shared memory**: Multi-agent shared stores are an open question. *(v1, retained)*

3. **Procedural memory update without catastrophic forgetting**: *(v1, retained)*

4. **Quantitative capacity and latency targets**: No quantitative benchmarks available. *(v1, retained)*

5. **Provenance-based confidence calibration**: *(v1, retained)*

6. **[v2] Taxonomy evolution validation automation**: The validation phase currently requires "domain experts" to review proposals. Research on automated validation — constraint checking, conflict detection, redundancy elimination — is needed before fully autonomous taxonomy evolution is safe. The current design supports both human-in-the-loop and rule-based validation but does not specify when each applies.

7. **[v2] Construction module refinement policies**: The construction/navigation separation (D22) requires policies for *when* and *how* to restructure the memory hierarchy based on retrieval failures. The research establishes that the construction module "iteratively refines how experiences are structured" but does not specify convergence criteria or restructure frequency.

8. **[v2] Cue-based retrieval performance at scale**: ACT-R's cue-based retrieval is well-grounded cognitively, but its computational performance when the chunk store grows large is an open implementation question. Indexing strategies for cue matching (inverted indexes, partial match acceleration) require empirical evaluation.

---

## 12. Assumptions (Updated)

1. **CoALA as canonical taxonomy** *(v1, retained)*

2. **Memory/knowledge as analytically distinct** *(v1, retained)*

3. **Cognitive science as design metaphor** *(v1, retained)*

4. **Interface-level specification only** *(v1, retained)*

5. **Single-agent scope** *(v1, retained)*

6. **Retrieval layer is synchronous in description** *(v1, retained)*

7. **Storage technology examples are illustrative** *(v1, retained)*

8. **[v2] SKOS as canonical taxonomy standard**: The W3C SKOS standard is treated as the authoritative reference for hierarchical knowledge organization, given its W3C standardization and widespread adoption in knowledge management. The SKOS property model (broader/narrower/related/collection/mapping) is adopted directly.

9. **[v2] Direct parent-child relations are sufficient for hierarchy**: The architecture assumes that storing only immediate parent-child (broader/narrower) relations and computing transitive closure at query time is sufficient for hierarchical navigation. This follows SKOS convention and avoids multi-parent complexity. Polyhierarchies (a concept with multiple parents) are supported via SKOS broader relations but are not the primary organizational model.

10. **[v2] Taxonomy evolution requires governance**: The architecture assumes that taxonomy evolution — particularly structural changes and merges — requires some form of governance (human review, validation rules, or confidence thresholds). Fully autonomous, unvalidated taxonomy evolution is considered unsafe for production agents. The validation phase is a mandatory gate.

11. **[v2] Cue-based retrieval is complementary, not replacement**: Cue-based retrieval (ACT-R-style slot-value matching) is added as a *fourth* retrieval mode alongside temporal, hierarchical, and semantic retrieval. It does not replace existing retrieval methods but provides a precise query mechanism for attribute-based lookups.

---

## 13. References (v2 — Including Hierarchical Taxonomy Research)

### v1 References (Retained)

| Source | Citation |
|--------|----------|
| CoALA framework | [arXiv:2309.02427](https://arxiv.org/abs/2309.02427) |
| Loop Engineering video | [YouTube](https://youtu.be/GrNbuWWJYiI) |
| MemGPT | [arXiv:2310.08560](https://arxiv.org/abs/2310.08560) |
| Schema theory / context binding | [Frontiers in Human Neuroscience](https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2023.1217093/full) |
| Episodic-neocortical dual-process | [arXiv:2605.17625](https://arxiv.org/html/2605.17625v1) |
| ACT-R / Soar | [arXiv:2201.09305](https://arxiv.org/abs/2201.09305); [arXiv:2205.03854](https://arxiv.org/pdf/2205.03854) |
| RAG limitations | [GoodData.AI](https://www.gooddata.ai/blog/from-rag-to-graphrag-knowledge-graphs-ontologies-and-smarter-ai/) |
| Knowledge graphs / ontologies | [Enterprise Knowledge](https://enterprise-knowledge.com/ontology-and-knowledge-graph-in-the-age-of-ai-and-agents/); [zbrain.ai](https://zbrain.ai/knowledge-graphs-for-agentic-ai/) |
| GraphRAG | [arXiv:2501.13958](https://arxiv.org/html/2501.13958v1) |
| Baddeley's working memory model | [Wikipedia](https://en.wikipedia.org/wiki/Baddeley%27s_model_of_working_memory) |
| Tulving's R/K distinction | [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S1053810009000804) |
| Predictive processing | [Predictably Correct](https://predictablycorrect.substack.com/p/a-predictive-processing-perspective) |
| Letta three-tier model | [arXiv:2606.24775](https://arxiv.org/html/2606.24775v1) |
| Neural-symbolic integration | [Neo4j blog](https://neo4j.com/blog/developer/knowledge-graph-structured-semantic-search/) |

### [v2] Hierarchical Taxonomy Research References

| Source | Citation |
|--------|----------|
| ACT-R Cognitive Architecture | [ACT-R Wikipedia](https://en.wikipedia.org/wiki/ACT-R); [PSU Paper](https://acs.ist.psu.edu/papers/ritterTOip.pdf) |
| Schema Theory | [ScienceDirect: Memory Schema](https://www.sciencedirect.com/topics/psychology/memory-schema); [Cognitive Psychology](https://www.cognitivepsychology.com/Schema_Theory) |
| Temporal Order Matters (temporal + hierarchical indexing) | [arXiv:2606.04555](https://arxiv.org/html/2606.04555) |
| Organize then Retrieve (construction + navigation) | [arXiv:2606.11680](https://arxiv.org/html/2606.11680v1) |
| Ontology-Constrained Neural Reasoning (evolution pipeline) | [arXiv:2604.00555](https://arxiv.org/html/2604.00555v5) |
| OntoDrift (semantic drift detection) | [CEUR-WS Vol-2821](https://ceur-ws.org/Vol-2821/paper1.pdf) |
| Automatic Ontology Generation | [arXiv:2201.05910](https://arxiv.org/pdf/2201.05910) |
| Ontology Learning Survey (concept formation) | [arXiv:2404.14991](https://arxiv.org/html/2404.14991v2) |
| Adaptive Ontology Evolution | [MDPI Applied Sciences](https://www.mdpi.com/2076-3417/11/22/10770) |
| W3C SKOS Reference | [W3C](https://www.w3.org/TR/skos-reference/) |
| W3C SKOS Primer | [W3C](https://www.w3.org/TR/skos-primer/) |
| ISKO Encyclopedia of Knowledge Organization | [ISKO](https://www.isko.org/cyclo/skos.htm) |
| SKOS Taxonomies (Hedden Information) | [Hedden](https://www.hedden-information.com/skos-taxonomies/) |
| Taxonomy vs Ontology (Dataversity) | [Dataversity](https://www.dataversity.net/articles/taxonomy-vs-ontology-machine-learning-breakthroughs/) |
| Taxonomies & ML (Forbes) | [Forbes](https://www.forbes.com/sites/cognitiveworld/2019/03/12/taxonomies-ontologies-and-machine-learning-the-future-of-knowledge-management/) |
