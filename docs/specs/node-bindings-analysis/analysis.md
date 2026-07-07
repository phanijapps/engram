# Node Bindings Structure Analysis

## Executive Summary

The 1472-line `bindings/node/src/lib.rs` file contains 7 distinct "engine" impl blocks with mixed responsibilities. While several engines are well-focused, **NativeKnowledgeEngine (~360 lines)** and **NativeIngestEngine (~251 lines)** exhibit god class characteristics and would benefit from modularization.

## File Structure Breakdown

### Helper Code (~211 lines)
- **Lines 42-51**: Request structs (TaxonomyValidationRequest, ConsolidationPlanRequest)
- **Lines 61-180**: `resolve_cross_file_edges()` function (~120 lines)
- **Lines 1028-1058**: Utility functions (id_field, scope_field, decode, encode, to_napi_error)

### Engine 1: NativeMemoryEngine (~65 lines)
**Responsibility**: Core memory operations (write, retrieve, forget)
- `new()` - Constructor with optional file path
- `write_memory_json()` - Write memory from JSON request
- `retrieve_json()` - Retrieve memory from JSON request  
- `forget_json()` - Forget memory from JSON request

**Assessment**: ✅ Well-focused, single responsibility, no refactoring needed

### Engine 2: NativeKnowledgeEngine (~360 lines) ⚠️ GOD CLASS
**Responsibility**: Knowledge graph, entities, relationships, taxonomy, ontology

**Methods (24 public functions)**:
- Graph operations: `graph_candidates_json`, `fuse_rrf_json`, `fuse_rrf_ids_json`
- Source operations: `put_source_json`
- Document operations: `put_document_json`
- Chunk operations: `put_chunk_json`, `get_chunk_json`
- Entity operations: `put_entity_json`, `get_entity_json`
- Relationship operations: `put_relationship_json`, `get_relationship_json`
- Graph operations: `put_graph_json`, `get_graph_json`, `neighbors_json`
- Concept operations: `put_concept_scheme_json`, `get_concept_scheme_json`, `put_concept_json`, `put_concept_relation_json`, `list_concepts_json`
- Taxonomy operations: `validate_taxonomy_proposal_json`
- Listing operations: `list_graphs_json`, `list_entities_json`, `list_relationships_json`, `list_chunks_json`, `list_sources_json`
- Ontology operations: `put_ontology_json`, `get_ontology_json`, `put_class_json`, `put_property_json`, `put_axiom_json`
- Validation: `validate_graph_json`

**Assessment**: ⚠️ **God class** - mixes 8 distinct responsibilities (graph, sources, documents, chunks, entities, relationships, concepts/ontology, taxonomy)

### Engine 3: NativeHierarchyEngine (~27 lines)
**Responsibility**: Hierarchy validation
- `new()` - Constructor
- `validate_parentage_json()` - Validate hierarchy parentage

**Assessment**: ✅ Well-focused, single responsibility

### Engine 4: NativeConsolidationEngine (~26 lines)
**Responsibility**: Consolidation planning
- `new()` - Constructor
- `plan_json()` - Plan consolidation operations

**Assessment**: ✅ Well-focused, single responsibility

### Engine 5: NativeEvalEngine (~27 lines)
**Responsibility**: Architecture evaluation
- `new()` - Constructor
- `architecture_coverage_json()` - Evaluate architecture coverage

**Assessment**: ✅ Well-focused, single responsibility

### Engine 6: NativeBeliefEngine (~87 lines)
**Responsibility**: Belief and contradiction management
- `new()` - Constructor
- Belief operations: `put_belief_json`, `list_beliefs_json`
- Contradiction operations: `put_contradiction_json`, `list_contradictions_json`, `get_contradiction_json`, `resolve_contradiction_json`, `detect_contradictions_json`

**Assessment**: ✅ Focused (2 related responsibilities - beliefs and contradictions)

### Engine 7: NativeIngestEngine (~251 lines) ⚠️ LARGE
**Responsibility**: Repository scanning and ingestion

**Methods (3 public functions + internal state)**:
- `new()` - Constructor
- `start_scan_job_json()` - Start async repository scan
- `get_scan_job_json()` - Get scan job status
- `ingest_extract_json()` - Ingest extracted content
- Internal: ScanJobState management (~130 lines of internal logic)

**Assessment**: ⚠️ Large but reasonably focused - main complexity is internal state management

## Refactoring Recommendations

### Priority 1: NativeKnowledgeEngine (~360 lines)

**Problem**: Mixes 8 distinct responsibilities in a single impl block

**Proposed Module Structure**:
```
knowledge/
  ├── mod.rs              # Main engine struct + constructor
  ├── graph.rs            # Graph operations (fuse, candidates, neighbors)
  ├── sources.rs          # Source operations
  ├── documents.rs        # Document operations
  ├── chunks.rs           # Chunk operations
  ├── entities.rs         # Entity operations
  ├── relationships.rs    # Relationship operations
  ├── concepts.rs         # Concept scheme operations
  ├── taxonomy.rs         # Taxonomy validation
  ├── ontology.rs         # Ontology operations
  └── listing.rs          # List operations (graphs, entities, etc.)
```

**Benefits**:
- Each module owns one clear responsibility
- Easier to test and maintain
- Follows pattern established in ingest adapter refactoring
- No changes to public API (internal refactoring only)

### Priority 2: NativeIngestEngine (~251 lines)

**Problem**: Internal state management mixed with public API

**Proposed Module Structure**:
```
ingest/
  ├── mod.rs              # Main engine struct + public API
  ├── scan_state.rs       # ScanJobState management (internal)
  └── operations.rs       # Scan and ingest operations
```

**Benefits**:
- Separates internal state from public interface
- Easier to understand async job management
- No changes to public API

### No Refactoring Needed

The following engines are well-focused and should remain as-is:
- **NativeMemoryEngine** (~65 lines) - Single responsibility
- **NativeHierarchyEngine** (~27 lines) - Single responsibility
- **NativeConsolidationEngine** (~26 lines) - Single responsibility  
- **NativeEvalEngine** (~27 lines) - Single responsibility
- **NativeBeliefEngine** (~87 lines) - Two related responsibilities (beliefs + contradictions)

## Implementation Strategy

If refactoring is approved, follow the same pattern as ingest adapter:

1. **Create module files** for each responsibility
2. **Move functions** to appropriate modules
3. **Update imports** in main engine file
4. **Re-export public functions** through mod.rs
5. **Run verification**: cargo check --workspace
6. **Run tests**: cargo test --package engram-node
7. **No changes** to TypeScript public API

## Risk Assessment

**Low Risk**: Internal refactoring only
- No changes to public N-API interface
- No changes to TypeScript bindings
- No changes to behavior, only code organization
- Same verification approach as ingest adapter refactoring

## Conclusion

The node bindings file contains **2 god classes** that would benefit from modularization:
1. **NativeKnowledgeEngine** (~360 lines) - Highest priority
2. **NativeIngestEngine** (~251 lines) - Medium priority

The remaining 5 engines are well-focused and do not require refactoring. The proposed module structure follows the established pattern from the ingest adapter refactoring, focusing on single responsibility per module.
