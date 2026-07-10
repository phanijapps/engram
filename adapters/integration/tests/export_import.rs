//! Integration tests for `SqlExportImport` (engram-host-sdk brief, S5).
//!
//! These tests exercise the SQLite `ExportImport` impl against in-memory
//! knowledge + memory stores. They mirror the block_on driving style of
//! `tests/batch_ingest.rs` — no tokio.
//!
//! Cases:
//! 1. Export from a seeded scope → `ImportData` with the correct record types
//!    and counts (knowledge sources/documents/chunks/entities/relationships +
//!    memory).
//! 2. Round-trip: export → `MigrationService::dry_run_import` →
//!    `apply_import` → Ok, with the validation row counts matching the export
//!    (parity through the validation-only migration pipeline).
//! 3. Scope isolation: a second tenant's records do not leak into another
//!    scope's export.
//! 4. Empty scope → empty `ImportData`.

use std::sync::Arc;

use engram_belief::BeliefRepository as _;
use engram_conformance::{SqlExportImport, SqlMigrationService};
use engram_domain::*;
use engram_hierarchy::HierarchyRepository as _;
use engram_integration::{ExportImport, MigrationManifest, MigrationService as _};
use engram_knowledge::{KnowledgeRepository as _, TaxonomyRepository as _};
use engram_memory::MemoryRepository as _;
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_hierarchy_sqlite::SqlHierarchyStore;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;

// ---------- helpers -------------------------------------------------------

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_string(),
        subject: Some(format!("subject-{tenant}")),
        workspace: Some(format!("workspace-{tenant}")),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("export-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Export Harness".to_string()),
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
        source: "export-test".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_string()),
    }
}

fn source(id: &str, scope: &Scope) -> KnowledgeSource {
    KnowledgeSource {
        id: Id::from(id),
        kind: SourceKind::Filesystem,
        scope: scope.clone(),
        name: format!("source-{id}"),
        uri: Some(format!("file:///repo/{id}")),
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn document(id: &str, source_id: &str) -> SourceDocument {
    SourceDocument {
        id: Id::from(id),
        source_id: Id::from(source_id),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some(format!("docs/{id}.md")),
        title: Some(format!("Document {id}")),
        mime_type: None,
        language: None,
        version: None,
        content_hash: format!("sha256:{id}"),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn chunk(id: &str, document_id: &str, source_id: &str) -> KnowledgeChunk {
    KnowledgeChunk {
        id: Id::from(id),
        document_id: Id::from(document_id),
        source_id: Id::from(source_id),
        kind: KnowledgeChunkKind::Paragraph,
        text: format!("chunk text {id}"),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: format!("sha256:{id}"),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn entity(id: &str, scope: &Scope) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind: EntityKind::Function,
        name: format!("entity-{id}"),
        aliases: Vec::new(),
        scope: scope.clone(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn relationship(id: &str, scope: &Scope) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from("export-entity-a")),
            kind: Some("function".to_string()),
            name: Some("caller".to_string()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(Id::from("export-entity-b")),
            kind: Some("function".to_string()),
            name: Some("callee".to_string()),
            aliases: Vec::new(),
        },
        scope: scope.clone(),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn memory_record(id: &str, text: &str, scope: &Scope) -> MemoryRecord {
    MemoryRecord {
        id: Id::from(id),
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
        scope: scope.clone(),
        provenance: provenance(),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn concept_scheme(id: &str, scope: &Scope) -> ConceptScheme {
    ConceptScheme {
        id: Id::from(id),
        uri: format!("https://example.com/schemes/{id}"),
        name: format!("scheme-{id}"),
        scope: scope.clone(),
        version: "1.0".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn concept(id: &str, scheme_id: &str) -> Concept {
    Concept {
        id: Id::from(id),
        uri: format!("https://example.com/concepts/{id}"),
        scheme_id: Id::from(scheme_id),
        pref_label: ConceptLabel {
            value: format!("concept-{id}"),
            language: Some("en".to_string()),
        },
        alt_labels: Vec::new(),
        definition: Some("export-test concept".to_string()),
        notation: None,
        status: ConceptStatus::Active,
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn belief(id: &str, scope: &Scope) -> Belief {
    Belief {
        id: Id::from(id),
        scope: scope.clone(),
        subject: BeliefSubject {
            key: format!("svc-{id}"),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: format!("belief-{id}"),
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
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn hierarchy_node(id: &str, scope: &Scope) -> HierarchyNode {
    HierarchyNode {
        id: HierarchyNodeId::from(id),
        scope: scope.clone(),
        kind: HierarchyNodeKind::Base,
        layer: 0,
        name: format!("node-{id}"),
        summary: None,
        parent_id: None,
        members: Vec::new(),
        source_target_type: None,
        source_target_id: None,
        embedding_refs: Vec::new(),
        status: HierarchyNodeStatus::Active,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

/// Seeds `tenant-a` with one source, one document, one chunk, two entities, one
/// relationship, one concept scheme + concept, one memory record, one belief,
/// and one hierarchy node. Returns the wired `SqlExportImport` (with belief +
/// hierarchy stores attached so those families are exported).
fn seeded_exporter() -> (
    SqlExportImport,
    Arc<SqlKnowledgeStore>,
    Arc<SqlMemoryService>,
    Arc<SqlBeliefStore>,
    Arc<SqlHierarchyStore>,
) {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let belief_store = Arc::new(SqlBeliefStore::open_in_memory().expect("belief open"));
    let hierarchy_store = Arc::new(SqlHierarchyStore::open_in_memory().expect("hierarchy open"));
    let scope_a = scope("tenant-a");

    block_on(knowledge.put_source(source("export-src", &scope_a))).expect("put source");
    block_on(knowledge.put_document(document("export-doc", "export-src"))).expect("put document");
    block_on(knowledge.put_chunk(chunk("export-chunk", "export-doc", "export-src")))
        .expect("put chunk");
    block_on(knowledge.put_entity(entity("export-entity-a", &scope_a))).expect("put entity a");
    block_on(knowledge.put_entity(entity("export-entity-b", &scope_a))).expect("put entity b");
    block_on(knowledge.put_relationship(relationship("export-rel", &scope_a))).expect("put rel");
    block_on(knowledge.put_concept_scheme(concept_scheme("export-scheme", &scope_a)))
        .expect("put scheme");
    block_on(knowledge.put_concept(concept("export-concept", "export-scheme")))
        .expect("put concept");
    block_on(memory.put_memory(memory_record("export-mem", "exported fact", &scope_a)))
        .expect("put memory");
    block_on(belief_store.put_belief(belief("export-belief", &scope_a))).expect("put belief");
    block_on(hierarchy_store.put_node(hierarchy_node("export-node", &scope_a))).expect("put node");

    let exporter = SqlExportImport::new(knowledge.clone(), memory.clone())
        .with_belief(belief_store.clone())
        .with_hierarchy(hierarchy_store.clone());
    (exporter, knowledge, memory, belief_store, hierarchy_store)
}

// ---------- tests ---------------------------------------------------------

#[test]
fn export_reads_seeded_scope_with_correct_counts() {
    let (exporter, _knowledge, _memory, _belief, _hierarchy) = seeded_exporter();
    let scope_a = scope("tenant-a");
    let data = block_on(exporter.export(&scope_a)).expect("export");

    // Knowledge family.
    assert_eq!(data.knowledge_sources.len(), 1, "one source");
    assert_eq!(data.knowledge_documents.len(), 1, "one document");
    assert_eq!(data.knowledge_chunks.len(), 1, "one chunk");
    assert_eq!(data.knowledge_entities.len(), 2, "two entities");
    assert_eq!(data.knowledge_relationships.len(), 1, "one relationship");
    // Taxonomy family.
    assert_eq!(data.concept_schemes.len(), 1, "one concept scheme");
    assert_eq!(data.concepts.len(), 1, "one concept");
    // Memory family.
    assert_eq!(data.memories.len(), 1, "one memory");
    // Belief family.
    assert_eq!(data.beliefs.len(), 1, "one belief");
    // Hierarchy family.
    assert_eq!(data.hierarchy_nodes.len(), 1, "one hierarchy node");
    // Vectors remain deferred (no scope-wide list method).
    assert!(data.vectors.is_empty(), "vectors not in v1 export");

    // The exported memory carries the seeded content.
    assert_eq!(data.memories[0].content, "exported fact");
    // The source carries its kind + uri.
    assert_eq!(data.knowledge_sources[0].source_type, "filesystem");
    assert_eq!(data.knowledge_sources[0].uri, "file:///repo/export-src");
    // The chunk carries its text.
    assert_eq!(data.knowledge_chunks[0].content, "chunk text export-chunk");
    // The relationship maps subject/object entity ids + predicate.
    assert_eq!(data.knowledge_relationships[0].source_id, "export-entity-a");
    assert_eq!(data.knowledge_relationships[0].target_id, "export-entity-b");
    assert_eq!(data.knowledge_relationships[0].kind, "calls");
    // The concept scheme maps to its title (name) + scope.
    assert_eq!(data.concept_schemes[0].title, "scheme-export-scheme");
    assert!(
        data.concept_schemes[0].scope.contains("tenant-a"),
        "concept scheme scope carries the tenant"
    );
    // The concept carries its scheme id + preferred label.
    assert_eq!(data.concepts[0].scheme_id, "export-scheme");
    assert_eq!(data.concepts[0].label, "concept-export-concept");
    // The belief carries its content + a non-empty scope + start_time.
    assert_eq!(data.beliefs[0].content, "belief-export-belief");
    assert!(
        data.beliefs[0].scope.contains("tenant-a"),
        "belief scope carries the tenant"
    );
    assert_ne!(data.beliefs[0].start_time, 0, "belief start_time is set");
    // The hierarchy node maps to its label + kind + scope.
    assert_eq!(data.hierarchy_nodes[0].label, "node-export-node");
    assert_eq!(
        data.hierarchy_nodes[0].kind.as_deref(),
        Some("base"),
        "hierarchy node kind maps to the serde enum string"
    );
    assert!(
        data.hierarchy_nodes[0].scope.contains("tenant-a"),
        "hierarchy node scope carries the tenant"
    );
}

#[test]
fn export_round_trips_through_migration_service() {
    let (exporter, _knowledge, _memory, _belief, _hierarchy) = seeded_exporter();
    let scope_a = scope("tenant-a");
    let data = block_on(exporter.export(&scope_a)).expect("export");

    // dry_run over the exported payload — row counts must match the export.
    let migration = SqlMigrationService::new(384);
    let report = migration.dry_run_import(&data).expect("dry_run");
    assert_eq!(report.row_counts.knowledge_sources, 1);
    assert_eq!(report.row_counts.knowledge_documents, 1);
    assert_eq!(report.row_counts.knowledge_chunks, 1);
    assert_eq!(report.row_counts.knowledge_entities, 2);
    assert_eq!(report.row_counts.knowledge_relationships, 1);
    assert_eq!(report.row_counts.memory, 1);
    assert_eq!(report.row_counts.concept_schemes, 1);
    assert_eq!(report.row_counts.concepts, 1);
    assert_eq!(report.row_counts.beliefs, 1);
    assert_eq!(report.row_counts.hierarchy_nodes, 1);
    // Scope translation succeeds (every record carries a non-empty tenant).
    assert_eq!(report.scope_translation.failed_count, 0);

    // apply_import with a fresh manifest succeeds (parity through the
    // validation-only migration pipeline).
    let manifest = MigrationManifest {
        validation_report: report,
        import_data: data,
    };
    migration.apply_import(&manifest).expect("apply");
}

#[test]
fn export_isolates_by_tenant_scope() {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let scope_a = scope("tenant-a");
    let scope_b = scope("tenant-b");

    // Seed one source + memory in each tenant.
    block_on(knowledge.put_source(source("src-a", &scope_a))).expect("put source a");
    block_on(knowledge.put_source(source("src-b", &scope_b))).expect("put source b");
    block_on(memory.put_memory(memory_record("mem-a", "fact a", &scope_a))).expect("put mem a");
    block_on(memory.put_memory(memory_record("mem-b", "fact b", &scope_b))).expect("put mem b");

    let exporter = SqlExportImport::new(knowledge, memory);
    let data_a = block_on(exporter.export(&scope_a)).expect("export a");
    let data_b = block_on(exporter.export(&scope_b)).expect("export b");

    // Each tenant sees only its own records — no cross-tenant leak.
    assert_eq!(data_a.knowledge_sources.len(), 1);
    assert_eq!(data_a.memories.len(), 1);
    assert_eq!(data_a.knowledge_sources[0].id, "src-a");
    assert_eq!(data_a.memories[0].content, "fact a");
    assert_eq!(data_b.knowledge_sources.len(), 1);
    assert_eq!(data_b.memories.len(), 1);
    assert_eq!(data_b.knowledge_sources[0].id, "src-b");
    assert_eq!(data_b.memories[0].content, "fact b");
}

#[test]
fn export_empty_scope_returns_empty_import_data() {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let exporter = SqlExportImport::new(knowledge, memory);
    let data = block_on(exporter.export(&scope("tenant-empty"))).expect("export");
    assert!(data.memories.is_empty());
    assert!(data.knowledge_sources.is_empty());
    assert!(data.knowledge_entities.is_empty());
}

#[test]
fn export_document_scope_inherited_from_owning_source() {
    // Documents and chunks carry no scope of their own; their scope is resolved
    // from their owning source. A document whose owning source is scope-visible
    // is exported with that source's scope string.
    let (exporter, _knowledge, _memory, _belief, _hierarchy) = seeded_exporter();
    let scope_a = scope("tenant-a");
    let data = block_on(exporter.export(&scope_a)).expect("export");
    // The document + chunk carry a non-empty scope resolved from the source.
    assert!(
        !data.knowledge_documents[0].scope.is_empty(),
        "document scope must be resolved from owning source"
    );
    assert!(
        data.knowledge_documents[0].scope.contains("tenant-a"),
        "document scope resolves to owning source's tenant"
    );
    assert!(
        data.knowledge_chunks[0].scope.contains("tenant-a"),
        "chunk scope resolves to owning source's tenant"
    );
}

#[test]
fn export_omits_belief_and_hierarchy_when_stores_not_attached() {
    // The belief + hierarchy stores are optional export inputs. An exporter
    // built without `with_belief` / `with_hierarchy` exports those families
    // empty even when a knowledge concept scheme is present (concept schemes
    // come from the always-wired knowledge store). This is the unwired-provider
    // path: a missing optional family is empty, never an error.
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let belief_store = Arc::new(SqlBeliefStore::open_in_memory().expect("belief open"));
    let hierarchy_store = Arc::new(SqlHierarchyStore::open_in_memory().expect("hierarchy open"));
    let scope_a = scope("tenant-a");

    // Seed a belief + hierarchy node, but do NOT attach the stores.
    block_on(belief_store.put_belief(belief("dangling-belief", &scope_a))).expect("put belief");
    block_on(hierarchy_store.put_node(hierarchy_node("dangling-node", &scope_a)))
        .expect("put node");
    // A concept scheme IS exported (knowledge store is always wired).
    block_on(knowledge.put_concept_scheme(concept_scheme("scheme-a", &scope_a)))
        .expect("put scheme");

    let exporter = SqlExportImport::new(knowledge, memory);
    let data = block_on(exporter.export(&scope_a)).expect("export");

    // Concept scheme is exported via the always-wired knowledge store.
    assert_eq!(
        data.concept_schemes.len(),
        1,
        "concept scheme exported via knowledge"
    );
    // Belief + hierarchy are empty because their stores were not attached.
    assert!(
        data.beliefs.is_empty(),
        "beliefs empty when belief store not attached"
    );
    assert!(
        data.hierarchy_nodes.is_empty(),
        "hierarchy empty when hierarchy store not attached"
    );
}
