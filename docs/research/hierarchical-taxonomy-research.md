# Hierarchical Taxonomy Research for AI Agent Memory/Knowledge Architecture

**Purpose**: Synthesize research on hierarchical memory organization, dynamic taxonomies, and hierarchical knowledge representation to inform the AI agent memory/knowledge architecture.

---

## Topic 1: Hierarchical Memory Organization

### Summary
Research from cognitive psychology and AI demonstrates that memory is not flat but hierarchically organized at multiple levels. ACT-R provides a chunk-based hierarchy model, Schema Theory shows episodic memories nest within conceptual frameworks, and recent AI research confirms that temporal + hierarchical indexing improves both storage and retrieval.

### Key Papers & Findings

#### ACT-R Cognitive Architecture (Anderson et al.)
- **Source**: [ACT-R Wikipedia Overview](https://en.wikipedia.org/wiki/ACT-R), [PSU Paper on ACT-R](https://acs.ist.psu.edu/papers/ritterTOip.pdf)
- **Key Finding**: "Chunks are not indexed in any way and cannot be accessed via their index or their memory address. The only way to access a chunk is by specifying a cue, which is a slot-value pair or a set of such pairs."
- **Implication**: ACT-R's declarative memory stores chunks as typed objects with slot-value pairs. Retrieval is cue-based, not address-based.

#### Schema Theory (Piaget, Bartlett, modern neuroscience)
- **Source**: [ScienceDirect: Memory Schema](https://www.sciencedirect.com/topics/psychology/memory-schema), [Schema Theory Cognitive Psychology](https://www.cognitivepsychology.com/Schema_Theory)
- **Key Finding**: Schemas possess four necessary features:
  1. An associative network structure
  2. Basis on multiple episodes
  3. Lack of unit detail
  4. Adaptability
- **Additional Features**:
  5. Chronological relationships
  6. Hierarchical organization
  7. Cross-connectivity
  8. Embedded response options
- **Quote**: "Schema theory proposes that knowledge is organized into structured mental frameworks — schemas — that represent our understanding of typical situations, events, and objects. Schemas guide perception (what we notice), memory (what we encode and recall), and inference (what we assume when data is missing)."

#### Hierarchical Memory for AI Agents
- **Source**: [Temporal Order Matters (arXiv:2606.04555)](https://arxiv.org/html/2606.04555)
- **Key Finding**: "The central takeaway is that conversational agents benefit from memory indexes that are both temporal and hierarchical: temporal order helps construct coherent memory states, while the tree structure lets retrieval select context at the appropriate granularity."
- **Source**: [Organize then Retrieve (arXiv:2606.11680)](https://arxiv.org/html/2606.11680v1)
- **Key Finding**: "The construction module iteratively refines how experiences are structured by distinguishing between failures caused by missing information and those caused by misleading or overloaded context. The navigation module retrieves task-relevant context by traversing the hierarchy using a lightweight agent trained with reinforcement learning to select minimal yet sufficient context."

#### CoALA Framework (Sumers et al., Princeton)
- **Source**: [Cognitive Architectures for Language Agents (arXiv:2309.02427)](https://arxiv.org/pdf/2309.02427)
- **Key Finding**: CoALA structures language agents into modular memory components: working memory, episodic memory, semantic memory, and procedural memory. The framework contextualizes language agents within the broader history of AI.

### Actionable Insights for Architecture

1. **Chunk Hierarchy**: Use typed chunks with slot-value pairs, accessible only via cue-based retrieval (not indexing)
2. **Episodic Nesting**: Structure episodes within conceptual schemas; each episode references its parent schema
3. **Multi-Level Indexing**: Implement both temporal ordering AND hierarchical tree structure for memory retrieval
4. **Construction + Navigation**: Separate memory construction (structuring) from retrieval (navigation)
5. **Adaptive Refinement**: Iteratively refine memory structure based on retrieval failures

---

## Topic 2: Dynamic Taxonomies

### Summary
Taxonomies and ontologies in AI systems must evolve as agents encounter new concepts. Research shows that dynamic ontology evolution involves discovery, proposal, validation, and merging phases. Semantic drift detection and adaptive learning enable taxonomies to remain current.

### Key Papers & Findings

#### Ontology Evolution in Agentic Systems
- **Source**: [Ontology-Constrained Neural Reasoning (arXiv:2604.00555)](https://arxiv.org/html/2604.00555v5)
- **Key Finding**: L5 ontology evolution involves:
  - **Discovery**: Agents encounter concepts outside current ontology
  - **Proposal**: System proposes extensions from recurring unrecognized concepts
  - **Validation**: Domain experts review and approve
  - **Merging**: Approved extensions integrated with provenance tracking

#### OntoDrift: Semantic Drift Detection
- **Source**: [OntoDrift (CEUR-WS Vol-2821)](https://ceur-ws.org/Vol-2821/paper1.pdf)
- **Key Finding**: "The semantic drift is evaluated at the concept level, by considering the main features involved in an ontology concept (e.g., intention, extension, labels, URIs, etc.) and at the structural level, by inspecting the taxonomic relations among concepts."

#### Dynamic Ontology Generation
- **Source**: [Automatic Ontology Generation Framework (arXiv:2201.05910)](https://arxiv.org/pdf/2201.05910)
- **Key Finding**: "KGs are generated at runtime and the data is current. Evolution is very likely in KGs for several reasons: (i) KGs represent dynamic environments..."

#### Ontology Learning for LLMs
- **Source**: [Ontology Learning Survey (arXiv:2404.14991)](https://arxiv.org/html/2404.14991v2)
- **Key Finding**: "Concept formation: Once terms and their synonyms are extracted, the next step is to group them into meaningful concepts or classes. This involves organizing related terms into hierarchies or categories based on their similarities, functionalities, or semantic relations."
- **Taxonomic Relations**: "Taxonomic relations establish hierarchical relationships between concepts, defining the 'is-a' relationship (e.g., 'car' is a 'vehicle')."

#### Adaptive Ontology Evolution
- **Source**: [Automatic Ontology-Based Model Evolution (MDPI Applied Sciences)](https://www.mdpi.com/2076-3417/11/22/10770)
- **Key Finding**: Metadata loops extract sentences containing partially refined local ontology concepts. Semantic analysis identifies main components (nouns, verbs) to label non-taxonomic relations.

### Actionable Insights for Architecture

1. **Discovery → Proposal → Validation → Merge Pipeline**: Implement lifecycle for new concept integration
2. **Semantic Drift Detection**: Monitor concept-level features (labels, extensions, URIs) and structural changes
3. **Provenance Tracking**: Track the origin of all taxonomy additions/modifications
4. **Concept Formation Pipeline**: Extract → Group → Hierarchize → Relate
5. **Multi-Level Evolution**: Allow both concept additions and structural reorganization

---

## Topic 3: Hierarchical Taxonomies (SKOS & Multi-Level Ontologies)

### Summary
The W3C SKOS standard provides the canonical approach for hierarchical knowledge organization in AI systems. It defines concept schemes, broader/narrower relations, and associative links that enable multi-level, cross-linked hierarchies suitable for AI agent knowledge representation.

### Key Papers & Findings

#### SKOS (Simple Knowledge Organization System)
- **Source**: [W3C SKOS Reference](https://www.w3.org/TR/skos-reference/), [SKOS Primer](https://www.w3.org/TR/skos-primer/)
- **Core Concepts**:
  - **Concept Scheme**: "A collection of concepts. A concept scheme is a single controlled vocabulary, thesaurus, hierarchical taxonomy, facet within a faceted taxonomy, or metadata property within a larger metadata schema."
  - **Broader/Narrower Relations**: "The properties skos:broader and skos:narrower are used to assert a direct hierarchical link between two SKOS concepts."
  - **Convention**: "By convention, skos:broader is only used to assert an immediate (i.e. direct) hierarchical link between two conceptual resources. Narrower concepts are typically rendered as children in a concept hierarchy (tree)."
  - **Semantic Relations**: "SKOS Core includes four properties for expressing paradigmatic semantic relationships between concepts: skos:semanticRelation, skos:broader, skos:narrower, skos:related."

#### SKOS Implementation Patterns
- **Source**: [ISKO Encyclopedia of KO](https://www.isko.org/cyclo/skos.htm), [SKOS Taxonomies - Hedden Information](https://www.hedden-information.com/skos-taxonomies/)
- **Key Finding**: SKOS concepts can be:
  - Identified using URIs
  - Labeled with lexical strings in multiple languages
  - Assigned notations (lexical codes)
  - Documented with various types of notes
  - Linked to other concepts via hierarchical and associative relations
  - Aggregated into concept schemes
  - Grouped into labeled and/or ordered collections
  - Mapped to concepts in other schemes

#### AI & Taxonomy Integration
- **Source**: [Dataversity: Taxonomy vs Ontology](https://www.dataversity.net/articles/taxonomy-vs-ontology-machine-learning-breakthroughs/), [Forbes: Taxonomies & ML](https://www.forbes.com/sites/cognitiveworld/2019/03/12/taxonomies-ontologies-and-machine-learning-the-future-of-knowledge-management/)
- **Key Finding**: "By using taxonomies and ontologies, machines make 'statistical inferences or statistical associations, based on proximity.' As new inputs enter the AI system, it adapts and modifies its behavior."
- **Trade-off**: "Working from a closed core ontology usually gives a more consistent mechanism for matching, but it also requires more discipline... Using a folksonomy... is less precise and controlled, but works better when you have fewer (or less experienced) taxonomists working with the data."

### Actionable Insights for Architecture

1. **SKOS-Aligned Schema**: Use URIs for concepts, multi-language labels, broader/narrower relations
2. **Concept Schemes**: Group related concepts into schemes (e.g., domain-specific taxonomies)
3. **Immediate Hierarchical Links**: Only assert direct parent-child relationships (trees, not graphs)
4. **Collections**: Use labeled collections for non-hierarchical groupings (facets, topics)
5. **Cross-Scheme Mapping**: Enable mapping to concepts in other schemes
6. **Controlled Vocabulary**: Maintain consistency via governance while allowing evolution

---

## Cross-Cutting Synthesis

### Integration Points

| Research Area | Key Insight | Architecture Implication |
|---------------|-------------|--------------------------|
| ACT-R Chunks | Cue-based retrieval, slot-value pairs | Knowledge chunks with typed attributes |
| Schema Theory | Hierarchical organization, cross-connectivity | Episodic memories nested in schemas |
| CoALA | Modular memory components | Separate working/semantic/episodic/procedural |
| Temporal Trees | Temporal + hierarchical indexing | Dual-index memory system |
| SKOS | Concept schemes, broader/narrower | Knowledge taxonomy structure |
| Ontology Evolution | Discovery → Proposal → Validation → Merge | Taxonomy lifecycle management |
| Semantic Drift | Concept + structural monitoring | Drift detection subsystem |

### Unified Framework Recommendations

1. **Memory Hierarchy**: Implement 4-level episodic nesting:
   - Raw event → Episode → Schema → Domain ontology

2. **Knowledge Hierarchy**: SKOS-aligned concept schemes:
   - Concepts → Collections → Concept Schemes → Cross-scheme mappings

3. **Retrieval Strategy**: Multi-dimensional querying:
   - Temporal index (chronological)
   - Hierarchical index (broader/narrower)
   - Semantic index (related/associative)
   - Cue-based retrieval (slot-value matching)

4. **Evolution Management**: Automated taxonomy lifecycle:
   - Monitor for new concepts (discovery)
   - Propose structural changes (proposal)
   - Validate with constraints (validation)
   - Merge with provenance (merge)

---

## Source URLs

### Hierarchical Memory
- [ACT-R Wikipedia](https://en.wikipedia.org/wiki/ACT-R)
- [PSU ACT-R Paper](https://acs.ist.psu.edu/papers/ritterTOip.pdf)
- [ScienceDirect: Memory Schema](https://www.sciencedirect.com/topics/psychology/memory-schema)
- [Schema Theory - Cognitive Psychology](https://www.cognitivepsychology.com/Schema_Theory)
- [Temporal Order Matters (arXiv:2606.04555)](https://arxiv.org/html/2606.04555)
- [Organize then Retrieve (arXiv:2606.11680)](https://arxiv.org/html/2606.11680v1)
- [CoALA Paper (arXiv:2309.02427)](https://arxiv.org/pdf/2309.02427)

### Dynamic Taxonomies
- [Ontology-Constrained Neural Reasoning (arXiv:2604.00555)](https://arxiv.org/html/2604.00555v5)
- [OntoDrift (CEUR-WS)](https://ceur-ws.org/Vol-2821/paper1.pdf)
- [Automatic Ontology Generation (arXiv:2201.05910)](https://arxiv.org/pdf/2201.05910)
- [Ontology Learning Survey (arXiv:2404.14991)](https://arxiv.org/html/2404.14991v2)
- [Dynamic Ontology Evolution (MDPI)](https://www.mdpi.com/2076-3417/11/22/10770)

### Hierarchical Taxonomies (SKOS)
- [W3C SKOS Reference](https://www.w3.org/TR/skos-reference/)
- [W3C SKOS Primer](https://www.w3.org/TR/skos-primer/)
- [ISKO Encyclopedia of KO](https://www.isko.org/cyclo/skos.htm)
- [Dataversity: Taxonomy vs Ontology](https://www.dataversity.net/articles/taxonomy-vs-ontology-machine-learning-breakthroughs/)
- [Forbes: Taxonomies & ML](https://www.forbes.com/sites/cognitiveworld/2019/03/12/taxonomies-ontologies-and-machine-learning-the-future-of-knowledge-management/)