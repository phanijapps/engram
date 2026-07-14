//! Inlined conformance checks for `bootstrap_sqlite`.
//!
//! Each capability family is gated by a lightweight, deterministic round-trip
//! against an **in-memory** store (never the file-backed store the provider
//! hands out, so conformance data never pollutes the real database). A family
//! flips to `Supported` only when its check returns `true`; otherwise it stays
//! `Unsupported { ConformanceFailed }` with no handle, so a broken adapter can
//! never be reached through the facade.
//!
//! These checks inline the key assertions from the adapters-layer conformance
//! fixtures (`adapters/integration/src/fixtures/`), which keep the full
//! edge-case suite (scope-isolation, dedup, partial-failure, temporal queries,
//! …) for the dedicated conformance test harness. The checks here are the
//! runtime gate: "does this family's adapter work end-to-end?"
//!
//! ADR-0022: engine-specific (names `Sql*`, opens SQLite in-memory). The module
//! lives under `src/sqlite/` behind the `sqlite` feature and is intentionally
//! exempt from the engine-neutrality gate.

use std::sync::Arc;

use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::*;
use engram_hierarchy::HierarchyRepository;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::MemoryService;
use engram_retrieval::VectorIndex;
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_hierarchy_sqlite::SqlHierarchyStore;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;
use engram_store_vector::SqliteVectorIndex;
use futures::executor::block_on;

use crate::sqlite::{
    SqlBatchIngest, SqlExportImport, SqlMigrationService, SqlObservability, SqlProvenanceQuery,
    SqlUnifiedRecall,
};
use crate::{
    BatchIngest as _, BatchIngestRequest, ExportImport as _, Observability as _,
    ProvenanceQuery as _, UnifiedRecall as _,
};
use crate::{MigrationService as _, compute_manifest_fingerprint};

// ----------------------------- public gate API ----------------------------

/// Memory family: write → retrieve → scope isolation against an in-memory store.
pub(crate) fn memory_ok() -> bool {
    let Ok(svc) = SqlMemoryService::open_in_memory() else {
        return false;
    };
    let req = write_request("tenant-a");
    let Ok(stored) = block_on(svc.write_memory(req)) else {
        return false;
    };
    let id = stored.record.id.to_string();
    let Ok(visible) = block_on(svc.retrieve(retrieve_request("tenant-a"))) else {
        return false;
    };
    if !visible.items.iter().any(|r| r.target_id == id) {
        return false;
    }
    // Scope isolation: tenant-b must not see tenant-a's memory.
    let Ok(hidden) = block_on(svc.retrieve(retrieve_request("tenant-b"))) else {
        return false;
    };
    !hidden.items.iter().any(|r| r.target_id == id)
}

/// Knowledge family: source → document → chunk round-trip + scope isolation.
pub(crate) fn knowledge_ok() -> bool {
    let Ok(store) = SqlKnowledgeStore::open_in_memory() else {
        return false;
    };
    let source_id = Id::from("source-1");
    let document_id = Id::from("document-1");
    let chunk_id = Id::from("chunk-1");
    let res = (|| -> Result<(), ()> {
        block_on(store.put_source(source(source_id.clone()))).map_err(|_| ())?;
        block_on(store.put_document(document(document_id.clone(), source_id.clone())))
            .map_err(|_| ())?;
        block_on(store.put_chunk(chunk(chunk_id.clone(), document_id.clone(), source_id)))
            .map_err(|_| ())?;
        Ok(())
    })();
    if res.is_err() {
        return false;
    }
    let visible = block_on(store.get_chunk(&chunk_id, &scope("tenant-a")))
        .ok()
        .flatten();
    let hidden = block_on(store.get_chunk(&chunk_id, &scope("tenant-b")))
        .ok()
        .flatten();
    visible.is_some() && hidden.is_none()
}

/// Graph family: graph → entity → relationship → neighbors + scope isolation.
pub(crate) fn graph_ok() -> bool {
    let Ok(store) = SqlKnowledgeStore::open_in_memory() else {
        return false;
    };
    let graph_id = Id::from("graph-1");
    let function_a = Id::from("function-a");
    if block_on(store.put_graph(graph(graph_id.clone()))).is_err() {
        return false;
    }
    if block_on(store.put_entity(entity(function_a.clone(), graph_id.clone()))).is_err() {
        return false;
    }
    if block_on(store.put_relationship(relationship(
        Id::from("rel-1"),
        function_a.clone(),
        Id::from("function-b"),
        graph_id.clone(),
    )))
    .is_err()
    {
        return false;
    }
    let visible = block_on(store.get_graph(&graph_id, &scope("tenant-a")))
        .ok()
        .flatten();
    let hidden = block_on(store.get_graph(&graph_id, &scope("tenant-b")))
        .ok()
        .flatten();
    let neighbors = block_on(store.neighbors(&graph_id, &function_a, &scope("tenant-a"), Some(1)))
        .map(|v| v.len())
        .unwrap_or(usize::MAX);
    visible.is_some() && hidden.is_none() && neighbors == 1
}

/// Ontology family: put → get + scope isolation.
pub(crate) fn ontology_ok() -> bool {
    let Ok(store) = SqlKnowledgeStore::open_in_memory() else {
        return false;
    };
    let ontology_id = Id::from("ontology-1");
    if block_on(store.put_ontology(ontology(ontology_id.clone()))).is_err() {
        return false;
    }
    let visible = block_on(store.get_ontology(&ontology_id, &scope("tenant-a")))
        .ok()
        .flatten();
    let hidden = block_on(store.get_ontology(&ontology_id, &scope("tenant-b")))
        .ok()
        .flatten();
    visible.is_some() && hidden.is_none()
}

/// Taxonomy family: concept scheme → concepts → list.
pub(crate) fn taxonomy_ok() -> bool {
    let Ok(store) = SqlKnowledgeStore::open_in_memory() else {
        return false;
    };
    let scheme_id = Id::from("scheme-1");
    if block_on(store.put_concept_scheme(concept_scheme(scheme_id.clone()))).is_err() {
        return false;
    }
    for concept_id in ["concept-rust", "concept-ts"] {
        if block_on(store.put_concept(concept(Id::from(concept_id), scheme_id.clone()))).is_err() {
            return false;
        }
    }
    let concepts =
        block_on(store.list_concepts(&scheme_id, &scope("tenant-a"))).unwrap_or_default();
    concepts.len() == 2
}

/// Belief family: valid-time lookup returns the correct belief per window.
pub(crate) fn belief_ok() -> bool {
    let Ok(store) = SqlBeliefStore::open_in_memory() else {
        return false;
    };
    let mut old = belief("belief-old", "svc-a", "old", 0.7);
    old.valid_from = Some(ts(10));
    old.valid_until = Some(ts(20));
    old.created_at = ts(10);
    let mut current = belief("belief-current", "svc-a", "current", 0.9);
    current.valid_from = Some(ts(20));
    current.created_at = ts(20);
    if block_on(store.put_belief(old)).is_err() || block_on(store.put_belief(current)).is_err() {
        return false;
    }
    let during_old = block_on(store.get_belief(BeliefQuery::live_subject(
        scope("tenant-a"),
        "svc-a",
        ts(15),
    )))
    .ok()
    .flatten();
    let during_current = block_on(store.get_belief(BeliefQuery::live_subject(
        scope("tenant-a"),
        "svc-a",
        ts(40),
    )))
    .ok()
    .flatten();
    during_old.is_some_and(|b| b.id == Id::from("belief-old"))
        && during_current.is_some_and(|b| b.id == Id::from("belief-current"))
}

/// Hierarchy family: 3-node chain → parent-path walk.
pub(crate) fn hierarchy_ok() -> bool {
    let Ok(store) = SqlHierarchyStore::open_in_memory() else {
        return false;
    };
    let res = (|| -> Result<(), ()> {
        block_on(store.put_node(node("a", 0, None))).map_err(|_| ())?;
        block_on(store.put_node(node("b", 1, Some("a")))).map_err(|_| ())?;
        block_on(store.put_node(node("c", 2, Some("b")))).map_err(|_| ())?;
        block_on(store.put_relation(relation("r1", "a", "b"))).map_err(|_| ())?;
        block_on(store.put_relation(relation("r2", "b", "c"))).map_err(|_| ())?;
        Ok(())
    })();
    if res.is_err() {
        return false;
    }
    let Ok(path) = block_on(store.path_for(&["c".to_string()], &scope("tenant-a"), None)) else {
        return false;
    };
    path.nodes.len() == 3
        && path.nodes[0].id == HierarchyNodeId::from("a")
        && path.nodes[2].id == HierarchyNodeId::from("c")
}

/// Provenance / episodes_evidence: embedded provenance recovers through the
/// `SqlProvenanceQuery` handle + scope isolation.
pub(crate) fn provenance_ok() -> bool {
    let Ok(raw) = SqlKnowledgeStore::open_in_memory() else {
        return false;
    };
    let store: Arc<SqlKnowledgeStore> = Arc::new(raw);
    let graph_id = Id::from("graph-prov");
    let entity_id = Id::from("entity-prov");
    let relationship_id = Id::from("relationship-prov");
    let source_id = Id::from("source-prov");
    let stable_source_key = source_id.to_string();
    let res = (|| -> Result<(), ()> {
        block_on(store.put_graph(prov_graph(graph_id.clone(), &stable_source_key)))
            .map_err(|_| ())?;
        block_on(store.put_entity(prov_entity(entity_id.clone(), graph_id.clone())))
            .map_err(|_| ())?;
        block_on(
            store.put_relationship(prov_relationship(relationship_id.clone(), graph_id.clone())),
        )
        .map_err(|_| ())?;
        block_on(store.put_source(prov_source(source_id))).map_err(|_| ())?;
        Ok(())
    })();
    if res.is_err() {
        return false;
    }
    let query = SqlProvenanceQuery::new(store);
    let scope_a = scope("tenant-a");
    let scope_b = scope("tenant-b");
    let entity_prov = block_on(query.provenance_for(
        EvidenceTargetType::Entity,
        &entity_id.to_string(),
        &scope_a,
    ))
    .ok()
    .flatten();
    let leaked = block_on(query.provenance_for(
        EvidenceTargetType::Entity,
        &entity_id.to_string(),
        &scope_b,
    ))
    .ok()
    .flatten();
    entity_prov.is_some_and(|p| p.confidence == Some(1.0)) && leaked.is_none()
}

/// Atomic batch: a full batch lands every writable record (best-effort,
/// `Complete`).
pub(crate) fn batch_ok() -> bool {
    let Ok(memory) = SqlMemoryService::open_in_memory() else {
        return false;
    };
    let Ok(knowledge) = SqlKnowledgeStore::open_in_memory() else {
        return false;
    };
    let memory: Arc<SqlMemoryService> = Arc::new(memory);
    let knowledge: Arc<SqlKnowledgeStore> = Arc::new(knowledge);
    let batch = SqlBatchIngest::new(memory.clone(), knowledge.clone());
    let request = BatchIngestRequest {
        idempotency_key: "bootstrap-batch".to_string(),
        scope: scope("tenant-batch"),
        source: Some(batch_source("src-batch")),
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![
            batch_fact("bootstrap fact one", "0"),
            batch_fact("bootstrap fact two", "1"),
        ],
        entities: vec![batch_entity("ent-batch-0")],
        relationships: vec![batch_relationship("rel-batch-0")],
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };
    block_on(batch.ingest(request))
        .map(|outcome| outcome.status == crate::BatchStatus::Complete)
        .unwrap_or(false)
}

/// Unified recall: constructs + runs against real in-memory memory/belief
/// stores (no retrieval lanes) and returns `Ok`.
pub(crate) fn recall_ok() -> bool {
    let Ok(memory_raw) = SqlMemoryService::open_in_memory() else {
        return false;
    };
    let Ok(belief_raw) = SqlBeliefStore::open_in_memory() else {
        return false;
    };
    let memory: Arc<dyn MemoryService> = Arc::new(memory_raw);
    let beliefs: Arc<dyn BeliefRepository> = Arc::new(belief_raw);
    let recall = SqlUnifiedRecall::new(memory, Vec::new(), beliefs);
    block_on(recall.recall(recall_request())).is_ok()
}

/// Export / import: seed a scope, export it, round-trip through the migration
/// service with matching row counts.
pub(crate) fn export_import_ok() -> bool {
    let knowledge = match SqlKnowledgeStore::open_in_memory() {
        Ok(s) => Arc::new(s),
        Err(_) => return false,
    };
    let memory = match SqlMemoryService::open_in_memory() {
        Ok(s) => Arc::new(s),
        Err(_) => return false,
    };
    let belief = match SqlBeliefStore::open_in_memory() {
        Ok(s) => Arc::new(s),
        Err(_) => return false,
    };
    let hierarchy = match SqlHierarchyStore::open_in_memory() {
        Ok(s) => Arc::new(s),
        Err(_) => return false,
    };
    let export_scope = export_scope();
    if seed_export(&knowledge, &memory, &belief, &hierarchy, &export_scope).is_err() {
        return false;
    }
    let exporter = SqlExportImport::new(knowledge, memory)
        .with_belief(belief)
        .with_hierarchy(hierarchy);
    let Ok(data) = block_on(exporter.export(&export_scope)) else {
        return false;
    };
    let migration = SqlMigrationService::new(384);
    let Ok(report) = migration.dry_run_import(&data) else {
        return false;
    };
    // Parity: every exported family counts > 0 (we seeded one of each) and the
    // dry-run row counts match the exported counts (memcmp via RowCounts Eq).
    report.row_counts.memory == 1
        && report.row_counts.knowledge_sources == 1
        && report.row_counts.beliefs == 1
        && report.row_counts.hierarchy_nodes == 1
}

/// Observability: seed + diagnostics snapshot carries delegated fields and
/// scope-derived counts.
pub(crate) fn observability_ok() -> bool {
    let knowledge = match SqlKnowledgeStore::open_in_memory() {
        Ok(s) => Arc::new(s),
        Err(_) => return false,
    };
    let beliefs = match SqlBeliefStore::open_in_memory() {
        Ok(s) => Arc::new(s),
        Err(_) => return false,
    };
    // Seed at least one entity + one belief under the diagnostic tenant.
    if block_on(knowledge.put_entity(entity(Id::from("obs-e"), Id::from("obs-g")))).is_err() {
        return false;
    }
    if block_on(beliefs.put_belief(belief("obs-belief", "svc-obs", "obs", 0.8))).is_err() {
        return false;
    }
    let capabilities = crate::CapabilityReport::new(CapabilityState::Supported);
    let embedding_config = crate::EmbeddingProviderConfig {
        provider_type: "test".to_string(),
        model: "test_model".to_string(),
        dimensions: 384,
        prompt_profile: "query".to_string(),
        normalization: None,
    };
    let obs = SqlObservability::new(
        Some(knowledge),
        Some(beliefs),
        diag_scope(),
        capabilities.clone(),
        embedding_config.clone(),
        "2026.01",
        "0.1.0",
    );
    let Ok(snap) = block_on(obs.diagnostics()) else {
        return false;
    };
    snap.capabilities == capabilities
        && snap.embedding_config == embedding_config
        && snap.schema_version == "2026.01"
        && snap.slow_query_diagnostics.is_none()
        && snap.record_counts.entities >= 1
        && snap.record_counts.beliefs >= 1
}

/// Migration: manifest fingerprint is deterministic and sensitive to changes.
pub(crate) fn migration_ok() -> bool {
    let counts = crate::RowCounts {
        memory: 2,
        knowledge_sources: 1,
        knowledge_documents: 1,
        knowledge_chunks: 3,
        knowledge_entities: 4,
        knowledge_relationships: 5,
        concept_schemes: 1,
        concepts: 6,
        beliefs: 2,
        contradictions: 0,
        hierarchy_nodes: 3,
        vectors: 7,
    };
    let hashes = vec![
        crate::record_key_hash("mem-1|tenant-a|100"),
        crate::record_key_hash("mem-2|tenant-a|101"),
    ];
    let fp1 = compute_manifest_fingerprint(&counts, &hashes);
    let fp2 = compute_manifest_fingerprint(&counts, &hashes);
    if fp1 != fp2 {
        return false;
    }
    let mut changed = counts.clone();
    changed.memory += 1;
    let fp_changed = compute_manifest_fingerprint(&changed, &hashes);
    fp1 != fp_changed
}

/// Retrieval trace: the extended `FusionTrace` round-trips through serde and a
/// vector retrieval yields a ranked hit.
pub(crate) fn retrieval_ok() -> bool {
    let trace = FusionTrace {
        query_id: Some("q-1".to_string()),
        vector_index: Some("conformance".to_string()),
        embedding_time_ms: Some(3),
        search_time_ms: Some(1),
        source: "vector.semantic".to_string(),
        source_rank: Some(1),
        source_score: Some(0.9),
        score: Some(0.9),
        rank: Some(1),
        fusion_strategy: Some(FusionStrategy::None),
        fusion_score: Some(0.9),
        rerank_strategy: Some(RerankStrategy::None),
        rerank_score: Some(0.9),
        discard_reason: None,
        deduplicated_with: Vec::new(),
    };
    let Ok(json) = serde_json::to_string(&trace) else {
        return false;
    };
    let Ok(parsed): Result<FusionTrace, _> = serde_json::from_str(&json) else {
        return false;
    };
    if parsed.query_id.as_deref() != Some("q-1") || parsed.rank != Some(1) {
        return false;
    }
    let dims = 4u32;
    let Ok(index) = SqliteVectorIndex::open_in_memory(dims) else {
        return false;
    };
    let space = EmbeddingSpace::new("conformance", "bge-small", dims, "query", None::<String>);
    let index = index.with_embedding_space(space.clone());
    let target = Id::from("chunk-1");
    if VectorIndex::insert(&index, &target, &space, vec![0.1, 0.2, 0.3, 0.4]).is_err() {
        return false;
    }
    let hits = VectorIndex::search(&index, &space, vec![0.1, 0.2, 0.3, 0.4], 1).unwrap_or_default();
    !hits.is_empty()
}

/// Vector family: insert/search round-trip + embedding-space / dimension
/// mismatch rejection.
pub(crate) fn vector_ok() -> bool {
    let dims = 4u32;
    let Ok(index) = SqliteVectorIndex::open_in_memory(dims) else {
        return false;
    };
    let space = EmbeddingSpace::new("conformance", "bge-small", dims, "query", None::<String>);
    let index = index.with_embedding_space(space.clone());
    let target = Id::from("chunk-1");
    if VectorIndex::insert(&index, &target, &space, vec![0.1, 0.2, 0.3, 0.4]).is_err() {
        return false;
    }
    let hits = VectorIndex::search(&index, &space, vec![0.1, 0.2, 0.3, 0.4], 1).unwrap_or_default();
    if hits.len() != 1 || hits[0].0 != target {
        return false;
    }
    let wrong_space = EmbeddingSpace::new("other-provider", "nomic", dims, "query", None::<String>);
    let insert_mismatch =
        VectorIndex::insert(&index, &target, &wrong_space, vec![0.1, 0.2, 0.3, 0.4]);
    let dim_mismatch = VectorIndex::insert(&index, &target, &space, vec![0.1, 0.2, 0.3]);
    insert_mismatch.is_err() && dim_mismatch.is_err()
}

// ----------------------------- shared constructors ------------------------

fn ts(seconds: i64) -> Timestamp {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_opt(seconds, 0)
        .single()
        .expect("timestamp")
}

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("conformance-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Conformance".to_owned()),
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance_base() -> Provenance {
    Provenance {
        source: "conformance".to_owned(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: Vec::new(),
        permissions: vec!["memory.write".to_string(), "memory.retrieve".to_string()],
        on_behalf_of: None,
    }
}

// ---- memory ----

fn write_request(tenant: &str) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: "conformance memory".to_string(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(tenant),
        requester: requester(),
        provenance: provenance_base(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn retrieve_request(tenant: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: "conformance".to_string(),
        scope: scope(tenant),
        requester: requester(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    }
}

// ---- knowledge / graph / taxonomy / ontology ----

fn source(id: Id) -> KnowledgeSource {
    KnowledgeSource {
        id,
        kind: SourceKind::Filesystem,
        scope: scope("tenant-a"),
        name: "docs".to_owned(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
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
        provenance: provenance_base(),
        policy: policy(),
        created_at: chrono::Utc::now(),
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
        provenance: provenance_base(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn graph(id: Id) -> KnowledgeGraph {
    KnowledgeGraph {
        id,
        scope: scope("tenant-a"),
        name: "Conformance Graph".to_owned(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
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
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
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
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn concept_scheme(id: Id) -> ConceptScheme {
    ConceptScheme {
        id,
        uri: "urn:scheme:conformance".to_owned(),
        name: "Conformance".to_owned(),
        scope: scope("tenant-a"),
        version: "1.0.0".to_owned(),
        provenance: provenance_base(),
        policy: policy(),
        created_at: chrono::Utc::now(),
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
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
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
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

// ---- belief ----

fn belief(id: &str, key: &str, content: &str, confidence: f32) -> Belief {
    Belief {
        id: Id::from(id),
        scope: scope("tenant-a"),
        subject: BeliefSubject {
            key: key.to_owned(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: content.to_owned(),
        status: BeliefStatus::Active,
        confidence,
        sources: Vec::new(),
        valid_from: None,
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: None,
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

// ---- hierarchy ----

fn node(id: &str, layer: u32, parent: Option<&str>) -> HierarchyNode {
    HierarchyNode {
        id: HierarchyNodeId::from(id),
        scope: scope("tenant-a"),
        kind: if layer == 0 {
            HierarchyNodeKind::Base
        } else {
            HierarchyNodeKind::Aggregate
        },
        layer,
        name: id.to_owned(),
        summary: None,
        parent_id: parent.map(HierarchyNodeId::from),
        members: Vec::new(),
        source_target_type: None,
        source_target_id: None,
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn relation(id: &str, source_id: &str, target_id: &str) -> HierarchyRelation {
    HierarchyRelation {
        id: id.to_owned(),
        scope: scope("tenant-a"),
        source_id: HierarchyNodeId::from(source_id),
        target_id: HierarchyNodeId::from(target_id),
        predicate: "parent_of".to_owned(),
        layer: None,
        strength: None,
        is_inter_cluster: None,
        evidence: Vec::new(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
    }
}

// ---- provenance ----

fn prov_graph(id: Id, stable_source_key: &str) -> KnowledgeGraph {
    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert(
        "stableSourceKey".to_string(),
        serde_json::Value::String(stable_source_key.to_string()),
    );
    KnowledgeGraph {
        id,
        scope: scope("tenant-a"),
        name: "Provenance Graph".to_owned(),
        uri: None,
        version: None,
        ontology_refs: Vec::new(),
        policy: policy(),
        provenance: provenance_with_evidence(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: Some(metadata),
    }
}

fn prov_entity(id: Id, graph_id: Id) -> KnowledgeEntity {
    KnowledgeEntity {
        id,
        graph_id: Some(graph_id),
        kind: EntityKind::Function,
        name: "provenanced_fn".to_owned(),
        aliases: Vec::new(),
        scope: scope("tenant-a"),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance_with_evidence(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn prov_relationship(id: Id, graph_id: Id) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id,
        graph_id: Some(graph_id),
        subject: EntityRef {
            id: Some(Id::from("entity-prov")),
            kind: Some("function".to_owned()),
            name: Some("caller".to_owned()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_owned(),
        object: EntityRef {
            id: Some(Id::from("entity-other")),
            kind: Some("function".to_owned()),
            name: Some("callee".to_owned()),
            aliases: Vec::new(),
        },
        scope: scope("tenant-a"),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance_with_evidence(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn prov_source(id: Id) -> KnowledgeSource {
    KnowledgeSource {
        id,
        kind: SourceKind::Filesystem,
        scope: scope("tenant-a"),
        name: "provenance source".to_owned(),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance_with_evidence(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn provenance_with_evidence() -> Provenance {
    let mut p = provenance_base();
    p.evidence = vec![EvidenceRef {
        target_type: EvidenceTargetType::Entity,
        target_id: Some("doc-1".to_string()),
        uri: None,
        quote: Some("supports the claim".to_string()),
        location: None,
    }];
    p
}

// ---- batch ----

fn batch_scope() -> Scope {
    scope("tenant-batch")
}

fn batch_source(id: &str) -> KnowledgeSource {
    KnowledgeSource {
        id: Id::from(id),
        kind: SourceKind::Filesystem,
        scope: batch_scope(),
        name: format!("source-{id}"),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn batch_fact(text: &str, suffix: &str) -> MemoryRecord {
    MemoryRecord {
        id: Id::from(format!("bootstrap-fact-{suffix}")),
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: text.to_string(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: batch_scope(),
        provenance: provenance_base(),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn batch_entity(id: &str) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind: EntityKind::Function,
        name: format!("entity-{id}"),
        aliases: Vec::new(),
        scope: batch_scope(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn batch_relationship(id: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from("ent-batch-0")),
            kind: Some("function".to_string()),
            name: Some("caller".to_string()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(Id::from("ent-batch-1")),
            kind: Some("function".to_string()),
            name: Some("callee".to_string()),
            aliases: Vec::new(),
        },
        scope: batch_scope(),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

// ---- recall ----

fn recall_request() -> RetrievalRequest {
    RetrievalRequest {
        query: "bootstrap-recall".to_string(),
        scope: Scope {
            tenant: "tenant-recall".to_string(),
            subject: Some("subject-recall".to_string()),
            workspace: Some("workspace-recall".to_string()),
            session: None,
            environment: Some("test".to_string()),
        },
        requester: Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: vec!["memory.retrieve".to_string()],
            on_behalf_of: None,
        },
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    }
}

// ---- export / import seed ----

fn export_scope() -> Scope {
    Scope {
        tenant: "tenant-export".to_string(),
        subject: Some("subject-export".to_string()),
        workspace: Some("workspace-export".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn seed_export(
    knowledge: &Arc<SqlKnowledgeStore>,
    memory: &Arc<SqlMemoryService>,
    belief: &Arc<SqlBeliefStore>,
    hierarchy: &Arc<SqlHierarchyStore>,
    scope: &Scope,
) -> Result<(), ()> {
    use engram_knowledge::KnowledgeRepository as _;
    use engram_memory::MemoryRepository as _;
    block_on(knowledge.put_source(KnowledgeSource {
        id: Id::from("fx-source"),
        kind: SourceKind::Filesystem,
        scope: scope.clone(),
        name: "fixture source".to_string(),
        uri: Some("file:///fixture".to_string()),
        version: None,
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(|_| ())?;
    block_on(knowledge.put_entity(KnowledgeEntity {
        id: Id::from("fx-entity-a"),
        graph_id: None,
        kind: EntityKind::Function,
        name: "fx-entity-a".to_string(),
        aliases: Vec::new(),
        scope: scope.clone(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }))
    .map_err(|_| ())?;
    block_on(memory.put_memory(MemoryRecord {
        id: Id::from("fx-mem-0"),
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: "fixture fact".to_string(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope.clone(),
        provenance: provenance_base(),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(|_| ())?;
    block_on(belief.put_belief(Belief {
        id: Id::from("fx-belief"),
        scope: scope.clone(),
        subject: BeliefSubject {
            key: "svc-fx".to_string(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: "fixture belief".to_string(),
        status: BeliefStatus::Active,
        confidence: 0.9,
        sources: Vec::new(),
        valid_from: Some(chrono::Utc::now()),
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: None,
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(|_| ())?;
    block_on(hierarchy.put_node(HierarchyNode {
        id: HierarchyNodeId::from("fx-node"),
        scope: scope.clone(),
        kind: HierarchyNodeKind::Base,
        layer: 0,
        name: "fixture node".to_string(),
        summary: None,
        parent_id: None,
        members: Vec::new(),
        source_target_type: None,
        source_target_id: None,
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(),
        provenance: provenance_base(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(|_| ())?;
    Ok(())
}

// ---- observability ----

fn diag_scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_string(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}
