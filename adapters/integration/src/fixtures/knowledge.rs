//! Knowledge, graph, taxonomy, and ontology capability fixtures.
//!
//! Each fixture exercises a scoped round-trip against the in-memory
//! `SqlKnowledgeStore` so the capability is only reported Supported when the
//! adapter actually persists and isolates by scope.

use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

use super::support::{policy, provenance, scope};

/// Runs the knowledge capability fixture: source → document → chunk round-trip.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if any knowledge write/read fails or scope leaks.
pub fn run_knowledge_fixture() -> CoreResult<()> {
    let store = SqlKnowledgeStore::open_in_memory()?;
    let source_id = Id::from("source-1");
    let document_id = Id::from("document-1");
    let chunk_id = Id::from("chunk-1");

    block_on(store.put_source(source(source_id.clone()))).map_err(err("put_source"))?;
    block_on(store.put_document(document(document_id.clone(), source_id.clone())))
        .map_err(err("put_document"))?;
    block_on(store.put_chunk(chunk(
        chunk_id.clone(),
        document_id.clone(),
        source_id.clone(),
    )))
    .map_err(err("put_chunk"))?;

    let visible =
        block_on(store.get_chunk(&chunk_id, &scope("tenant-a"))).map_err(err("get_chunk"))?;
    let hidden =
        block_on(store.get_chunk(&chunk_id, &scope("tenant-b"))).map_err(err("get_chunk"))?;
    if visible.is_none() || hidden.is_some() {
        return Err(err("scope_isolation")(CoreError::Conflict {
            reason: "chunk scope isolation failed".to_string(),
        }));
    }
    Ok(())
}

/// Runs the graph capability fixture: graph → entity → relationship → neighbors.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if graph traversal or scope isolation fails.
pub fn run_graph_fixture() -> CoreResult<()> {
    let store = SqlKnowledgeStore::open_in_memory()?;
    let graph_id = Id::from("graph-1");
    let function_a = Id::from("function-a");

    block_on(store.put_graph(graph(graph_id.clone()))).map_err(err("put_graph"))?;
    block_on(store.put_entity(entity(function_a.clone(), graph_id.clone())))
        .map_err(err("put_entity"))?;
    for (rel_id, object_id) in [("rel-1", "function-b"), ("rel-2", "function-c")] {
        block_on(store.put_relationship(relationship(
            Id::from(rel_id),
            function_a.clone(),
            Id::from(object_id),
            graph_id.clone(),
        )))
        .map_err(err("put_relationship"))?;
    }

    let visible =
        block_on(store.get_graph(&graph_id, &scope("tenant-a"))).map_err(err("get_graph"))?;
    let hidden =
        block_on(store.get_graph(&graph_id, &scope("tenant-b"))).map_err(err("get_graph"))?;
    let neighbors = block_on(store.neighbors(&graph_id, &function_a, &scope("tenant-a"), Some(1)))
        .map_err(err("neighbors"))?;
    let hidden_neighbors =
        block_on(store.neighbors(&graph_id, &function_a, &scope("tenant-b"), None))
            .map_err(err("neighbors"))?;

    if visible.is_none() || hidden.is_some() {
        return Err(err("scope_isolation")(CoreError::Conflict {
            reason: "graph scope isolation failed".to_string(),
        }));
    }
    if neighbors.len() != 1 || !hidden_neighbors.is_empty() {
        return Err(err("neighbors")(CoreError::Conflict {
            reason: "graph neighbor traversal incorrect".to_string(),
        }));
    }
    Ok(())
}

/// Runs the taxonomy capability fixture: concept scheme → concepts → relation.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if taxonomy write/read fails or scope leaks.
pub fn run_taxonomy_fixture() -> CoreResult<()> {
    let store = SqlKnowledgeStore::open_in_memory()?;
    let scheme_id = Id::from("scheme-1");

    block_on(store.put_concept_scheme(concept_scheme(scheme_id.clone())))
        .map_err(err("put_concept_scheme"))?;
    for concept_id in ["concept-rust", "concept-ts"] {
        block_on(store.put_concept(concept(Id::from(concept_id), scheme_id.clone())))
            .map_err(err("put_concept"))?;
    }

    let visible = block_on(store.get_concept_scheme(&scheme_id, &scope("tenant-a")))
        .map_err(err("get_scheme"))?;
    let hidden = block_on(store.get_concept_scheme(&scheme_id, &scope("tenant-b")))
        .map_err(err("get_scheme"))?;
    let concepts = block_on(store.list_concepts(&scheme_id, &scope("tenant-a")))
        .map_err(err("list_concepts"))?;
    let hidden_concepts = block_on(store.list_concepts(&scheme_id, &scope("tenant-b")))
        .map_err(err("list_concepts"))?;

    if visible.is_none() || hidden.is_some() {
        return Err(err("scope_isolation")(CoreError::Conflict {
            reason: "taxonomy scope isolation failed".to_string(),
        }));
    }
    if concepts.len() != 2 || !hidden_concepts.is_empty() {
        return Err(err("list_concepts")(CoreError::Conflict {
            reason: "concept listing incorrect".to_string(),
        }));
    }
    Ok(())
}

/// Runs the ontology capability fixture: ontology → class → property → axiom.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if ontology write/read fails or scope leaks.
pub fn run_ontology_fixture() -> CoreResult<()> {
    let store = SqlKnowledgeStore::open_in_memory()?;
    let ontology_id = Id::from("ontology-1");

    block_on(store.put_ontology(ontology(ontology_id.clone()))).map_err(err("put_ontology"))?;

    let visible = block_on(store.get_ontology(&ontology_id, &scope("tenant-a")))
        .map_err(err("get_ontology"))?;
    let hidden = block_on(store.get_ontology(&ontology_id, &scope("tenant-b")))
        .map_err(err("get_ontology"))?;
    if visible.is_none() || hidden.is_some() {
        return Err(err("scope_isolation")(CoreError::Conflict {
            reason: "ontology scope isolation failed".to_string(),
        }));
    }
    Ok(())
}

// ---------- domain constructors -------------------------------------------

fn graph(id: Id) -> KnowledgeGraph {
    KnowledgeGraph {
        id,
        scope: scope("tenant-a"),
        name: "Conformance Graph".to_owned(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn entity(id: Id, graph_id: Id) -> KnowledgeEntity {
    KnowledgeEntity {
        id,
        graph_id: Some(graph_id),
        kind: EntityKind::Function,
        name: "function_a".to_owned(),
        aliases: Vec::new(),
        scope: scope("tenant-a"),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn relationship(id: Id, subject: Id, object: Id, graph_id: Id) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id,
        graph_id: Some(graph_id),
        subject: EntityRef {
            id: Some(subject),
            kind: Some("function".to_owned()),
            name: Some("function_a".to_owned()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_owned(),
        object: EntityRef {
            id: Some(object),
            kind: Some("function".to_owned()),
            name: Some("function_b".to_owned()),
            aliases: Vec::new(),
        },
        scope: scope("tenant-a"),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
    }
}

fn source(id: Id) -> KnowledgeSource {
    KnowledgeSource {
        id,
        kind: SourceKind::Filesystem,
        scope: scope("tenant-a"),
        name: "docs".to_owned(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn document(id: Id, source_id: Id) -> SourceDocument {
    SourceDocument {
        id,
        source_id,
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some("docs/intro.md".to_owned()),
        title: None,
        mime_type: None,
        language: None,
        version: None,
        content_hash: "sha256:abc".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn chunk(id: Id, document_id: Id, source_id: Id) -> KnowledgeChunk {
    KnowledgeChunk {
        id,
        document_id,
        source_id,
        kind: KnowledgeChunkKind::Paragraph,
        text: "conformance chunk".to_owned(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: "sha256:chunk".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn concept_scheme(id: Id) -> ConceptScheme {
    ConceptScheme {
        id,
        uri: "urn:scheme:conformance".to_owned(),
        name: "Conformance".to_owned(),
        scope: scope("tenant-a"),
        version: "1.0.0".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: Utc::now(),
        updated_at: None,
    }
}

fn concept(id: Id, scheme_id: Id) -> Concept {
    let label = id.to_string();
    Concept {
        id,
        uri: format!("urn:concept:{label}"),
        scheme_id,
        pref_label: ConceptLabel {
            value: label,
            language: Some("en".to_owned()),
        },
        alt_labels: Vec::new(),
        definition: None,
        notation: None,
        status: ConceptStatus::Active,
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
    }
}

fn ontology(id: Id) -> Ontology {
    Ontology {
        id,
        uri: "urn:ontology:conformance".to_owned(),
        name: "Conformance Ontology".to_owned(),
        scope: scope("tenant-a"),
        language: OntologyLanguage::Owl,
        version: "1.0.0".to_owned(),
        status: OntologyStatus::Active,
        imports: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.knowledge".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_fixture_passes() {
        if let Err(e) = run_knowledge_fixture() {
            panic!("knowledge fixture failed: {e}");
        }
    }

    #[test]
    fn graph_fixture_passes() {
        if let Err(e) = run_graph_fixture() {
            panic!("graph fixture failed: {e}");
        }
    }

    #[test]
    fn taxonomy_fixture_passes() {
        assert!(run_taxonomy_fixture().is_ok());
    }

    #[test]
    fn ontology_fixture_passes() {
        assert!(run_ontology_fixture().is_ok());
    }
}
