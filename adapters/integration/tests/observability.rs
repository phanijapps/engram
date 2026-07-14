//! Integration tests for `SqlObservability` (engram-host-sdk brief, S6).
//!
//! These tests exercise the SQLite `Observability` impl against in-memory
//! knowledge + belief stores. They mirror the block_on driving style of
//! `tests/recall.rs` and `tests/provenance_query.rs` — no tokio.
//!
//! Cases:
//! 1. Seeded snapshot → correct record counts for the listable types
//!    (entities, relationships, sources, chunks, beliefs); `documents` and
//!    `memories` degrade to `0` (no list API in v1).
//! 2. Capabilities + embedding config + schema/adapter versions are present and
//!    passed through unchanged.
//! 3. `slow_query_diagnostics` is `None` in v1.
//! 4. Degraded mode: unwired (`None`) stores → every count `0`, snapshot still
//!    `Ok`.

use std::sync::Arc;

use engram_belief::BeliefRepository;
use engram_conformance::SqlObservability;
use engram_domain::*;
use engram_integration::{
    CapabilityReport, DiagnosticsSnapshot, EmbeddingProviderConfig, Observability,
};
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

const TENANT: &str = "tenant-obs";
const SCHEMA_VERSION: &str = "2026.01";
const ADAPTER_VERSION: &str = "0.1.0";

// ---------- scoped snapshot (case 1, 2, 3) --------------------------------

#[test]
fn seeded_snapshot_has_correct_counts_and_fields() {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge store"));
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("belief store"));
    seed_knowledge(&knowledge);
    seed_beliefs(&beliefs);

    let obs = SqlObservability::new(
        Some(knowledge),
        Some(beliefs),
        diag_scope(),
        sample_report(),
        sample_embedding_config(),
        SCHEMA_VERSION,
        ADAPTER_VERSION,
    );
    let snap = block_on(obs.diagnostics()).expect("diagnostics");

    // ---- Case 1: correct counts for listable types ----------------------
    assert_eq!(snap.record_counts.entities, 2, "entities");
    assert_eq!(snap.record_counts.relationships, 2, "relationships");
    assert_eq!(snap.record_counts.sources, 2, "sources");
    assert_eq!(snap.record_counts.chunks, 2, "chunks");
    assert_eq!(snap.record_counts.beliefs, 2, "beliefs");
    // Documents + memories have no list API in v1 → degrade to 0 (not an error).
    assert_eq!(snap.record_counts.documents, 0, "documents degrade to 0");
    assert_eq!(snap.record_counts.memories, 0, "memories degrade to 0");

    // ---- Case 2: capabilities + config + versions passed through --------
    assert_eq!(snap.capabilities, sample_report(), "capabilities delegated");
    assert_eq!(
        snap.embedding_config,
        sample_embedding_config(),
        "embedding config passed through"
    );
    assert_eq!(snap.schema_version, SCHEMA_VERSION);
    assert_eq!(snap.adapter_version, ADAPTER_VERSION);

    // ---- Case 3: slow-query diagnostics deferred in v1 ------------------
    assert!(
        snap.slow_query_diagnostics.is_none(),
        "slow_query_diagnostics is None in v1"
    );
}

#[test]
fn diagnostic_scope_sees_all_records_in_tenant() {
    // The broad diagnostic scope (tenant set, optional fields None) must see
    // records that were seeded under a fully-qualified scope in the same tenant.
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge store"));
    seed_knowledge(&knowledge);

    let obs = SqlObservability::new(
        Some(knowledge),
        None,
        diag_scope(),
        sample_report(),
        sample_embedding_config(),
        SCHEMA_VERSION,
        ADAPTER_VERSION,
    );
    let snap = block_on(obs.diagnostics()).expect("diagnostics");
    assert_eq!(
        snap.record_counts.entities, 2,
        "broad scope sees all entities"
    );
    assert_eq!(
        snap.record_counts.sources, 2,
        "broad scope sees all sources"
    );
}

#[test]
fn diagnostics_degrades_when_stores_unwired() {
    // Case 4: no stores wired → every count 0, snapshot still Ok, fields present.
    let obs = SqlObservability::new(
        None,
        None,
        diag_scope(),
        sample_report(),
        sample_embedding_config(),
        SCHEMA_VERSION,
        ADAPTER_VERSION,
    );
    let snap = block_on(obs.diagnostics()).expect("diagnostics must not error when unwired");
    let zero = engram_integration::RecordCounts::empty();
    assert_eq!(
        snap.record_counts, zero,
        "unwired stores report all-zero counts"
    );
    assert_eq!(snap.schema_version, SCHEMA_VERSION);
    assert_eq!(snap.adapter_version, ADAPTER_VERSION);
    assert!(snap.slow_query_diagnostics.is_none());
    let _: &DiagnosticsSnapshot = &snap; // type is reachable + populated
}

// ---------- seed helpers --------------------------------------------------

fn seed_knowledge(store: &Arc<SqlKnowledgeStore>) {
    let scope = seed_scope();
    // 2 sources.
    for sid in ["source-1", "source-2"] {
        block_on(store.put_source(source(Id::from(sid), scope.clone()))).expect("put_source");
    }
    // 1 graph + 2 entities + 2 relationships.
    let graph_id = Id::from("graph-1");
    block_on(store.put_graph(graph(graph_id.clone(), scope.clone()))).expect("put_graph");
    for eid in ["function-a", "function-b"] {
        block_on(store.put_entity(entity(Id::from(eid), graph_id.clone(), scope.clone())))
            .expect("put_entity");
    }
    block_on(store.put_relationship(relationship(
        Id::from("rel-1"),
        Id::from("function-a"),
        Id::from("function-b"),
        graph_id.clone(),
        scope.clone(),
    )))
    .expect("put_relationship");
    block_on(store.put_relationship(relationship(
        Id::from("rel-2"),
        Id::from("function-b"),
        Id::from("function-a"),
        graph_id,
        scope.clone(),
    )))
    .expect("put_relationship");
    // 2 chunks (visibility follows their source's scope; both sources are in scope).
    for (idx, source_id) in [(0, "source-1"), (1, "source-2")] {
        let doc_id = Id::from(format!("document-{idx}"));
        block_on(store.put_document(document(doc_id.clone(), Id::from(source_id), scope.clone())))
            .expect("put_document");
        block_on(store.put_chunk(chunk(
            Id::from(format!("chunk-{idx}")),
            doc_id,
            Id::from(source_id),
            scope.clone(),
        )))
        .expect("put_chunk");
    }
}

fn seed_beliefs(store: &Arc<SqlBeliefStore>) {
    let scope = seed_scope();
    for i in 1..=2 {
        block_on(store.put_belief(belief(&format!("belief-{i}"), scope.clone())))
            .expect("put_belief");
    }
}

// ---------- domain constructors -------------------------------------------

/// The fully-qualified scope records are seeded under.
fn seed_scope() -> Scope {
    Scope {
        tenant: TENANT.to_string(),
        subject: Some("subject-obs".to_string()),
        workspace: Some("workspace-obs".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

/// The broad diagnostic scope: same tenant, all optional fields `None`, so it
/// sees every record in the tenant regardless of subject/workspace/environment.
fn diag_scope() -> Scope {
    Scope {
        tenant: TENANT.to_string(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn source(id: Id, scope: Scope) -> KnowledgeSource {
    KnowledgeSource {
        id,
        kind: SourceKind::Filesystem,
        scope,
        name: "obs-source".to_string(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn document(id: Id, source_id: Id, _scope: Scope) -> SourceDocument {
    SourceDocument {
        id,
        source_id,
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some("docs/obs.md".to_string()),
        title: None,
        mime_type: None,
        language: None,
        version: None,
        content_hash: "sha256:obs".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn chunk(id: Id, document_id: Id, source_id: Id, _scope: Scope) -> KnowledgeChunk {
    KnowledgeChunk {
        id,
        document_id,
        source_id,
        kind: KnowledgeChunkKind::Paragraph,
        text: "obs chunk".to_string(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: "sha256:obs-chunk".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn graph(id: Id, scope: Scope) -> KnowledgeGraph {
    KnowledgeGraph {
        id,
        scope,
        name: "obs-graph".to_string(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn entity(id: Id, graph_id: Id, scope: Scope) -> KnowledgeEntity {
    let name = id.to_string();
    KnowledgeEntity {
        id,
        graph_id: Some(graph_id),
        kind: EntityKind::Function,
        name,
        aliases: Vec::new(),
        scope,
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn relationship(
    id: Id,
    subject: Id,
    object: Id,
    graph_id: Id,
    scope: Scope,
) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id,
        graph_id: Some(graph_id),
        subject: EntityRef {
            id: Some(subject),
            kind: Some("function".to_string()),
            name: None,
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(object),
            kind: Some("function".to_string()),
            name: None,
            aliases: Vec::new(),
        },
        scope,
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn belief(id: &str, scope: Scope) -> Belief {
    Belief {
        id: Id::from(id),
        scope,
        subject: BeliefSubject {
            key: format!("svc-{id}"),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: format!("belief {id}"),
        status: BeliefStatus::Active,
        confidence: 0.8,
        sources: Vec::new(),
        valid_from: Some(chrono::Utc::now()),
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: None,
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "observability-test".to_string(),
        actor: Actor {
            id: Id::from("obs-agent"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: None,
    }
}

fn sample_report() -> CapabilityReport {
    CapabilityReport::new(CapabilityState::Supported)
}

fn sample_embedding_config() -> EmbeddingProviderConfig {
    EmbeddingProviderConfig {
        provider_type: "test".to_string(),
        model: "test_model".to_string(),
        dimensions: 384,
        prompt_profile: "query".to_string(),
        normalization: None,
    }
}
