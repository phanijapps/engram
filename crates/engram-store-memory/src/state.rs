//! Process-local state for the in-memory adapter.
//!
//! The state shape is private to this crate. Public callers interact through
//! core repository and service traits, which keeps future SQL or vector adapters
//! free to choose different internal tables and indexes.

use std::collections::BTreeMap;

use engram_domain::{
    Belief, Contradiction, HierarchyNode, HierarchyRelation, KnowledgeChunk, KnowledgeEntity,
    KnowledgeGraph, KnowledgeRelationship, KnowledgeSource, MemoryEvent, MemoryRecord, Ontology,
    OntologyAxiom, OntologyClass, OntologyProperty, SourceDocument, WriteMemoryResponse,
};

#[derive(Debug, Default)]
pub(crate) struct InMemoryState {
    pub(crate) memories: BTreeMap<String, MemoryRecord>,
    pub(crate) events: Vec<MemoryEvent>,
    pub(crate) idempotency: BTreeMap<String, WriteMemoryResponse>,
    pub(crate) knowledge_sources: BTreeMap<String, KnowledgeSource>,
    pub(crate) source_documents: BTreeMap<String, SourceDocument>,
    pub(crate) knowledge_chunks: BTreeMap<String, KnowledgeChunk>,
    pub(crate) knowledge_entities: BTreeMap<String, KnowledgeEntity>,
    pub(crate) knowledge_relationships: BTreeMap<String, KnowledgeRelationship>,
    pub(crate) knowledge_graphs: BTreeMap<String, KnowledgeGraph>,
    pub(crate) ontologies: BTreeMap<String, Ontology>,
    pub(crate) ontology_classes: BTreeMap<String, OntologyClass>,
    pub(crate) ontology_properties: BTreeMap<String, OntologyProperty>,
    pub(crate) ontology_axioms: BTreeMap<String, OntologyAxiom>,
    pub(crate) hierarchy_nodes: BTreeMap<String, HierarchyNode>,
    pub(crate) hierarchy_relations: BTreeMap<String, HierarchyRelation>,
    pub(crate) beliefs: BTreeMap<String, Belief>,
    pub(crate) contradictions: BTreeMap<String, Contradiction>,
}
