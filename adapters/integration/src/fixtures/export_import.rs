//! Export / import capability fixture (engram-host-sdk brief, S5).
//!
//! Exercises [`SqlExportImport`] end-to-end against in-memory stores: seed a
//! scope with knowledge (source, document, chunk, entities, relationship) and
//! memory records, export it into one [`ImportData`], then round-trip the
//! payload through [`MigrationService::dry_run_import`] →
//! [`MigrationService::apply_import`] (the existing import handle) and verify
//! the validation row counts match the exported counts (parity).
//!
//! Import is validation-only in the current [`SqlMigrationService`]
//! ([`apply_import`] re-derives the manifest fingerprint and acknowledges a
//! fresh manifest), so "records recoverable with matching counts" is verified
//! through the validation pipeline: the exported [`ImportData`] carries the
//! right record types and counts, and `dry_run_import` reports those same
//! counts; a fresh manifest built from the report + the exported data is
//! accepted by `apply_import`.

use std::sync::Arc;

use engram_belief::BeliefRepository as _;
use engram_domain::*;
use engram_hierarchy::HierarchyRepository as _;
use engram_integration::sqlite::{SqlExportImport, SqlMigrationService};
use engram_integration::{ExportImport, MigrationManifest, MigrationService as _, RowCounts};
use engram_knowledge::{KnowledgeRepository as _, TaxonomyRepository as _};
use engram_memory::MemoryRepository as _;
use engram_runtime::{CoreError, CoreResult};
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_hierarchy_sqlite::SqlHierarchyStore;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;

/// Runs the export / import conformance fixture.
///
/// Seeds a scope, exports it, and round-trips the [`ImportData`] through the
/// migration service. Returns `Ok(())` when the exported counts match the
/// dry-run row counts and `apply_import` accepts the fresh manifest.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if any assertion fails or a store call errors.
pub fn run_export_import_fixture() -> CoreResult<()> {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().map_err(err("open knowledge"))?);
    let memory = Arc::new(SqlMemoryService::open_in_memory().map_err(err("open memory"))?);
    let belief = Arc::new(SqlBeliefStore::open_in_memory().map_err(err("open belief"))?);
    let hierarchy = Arc::new(SqlHierarchyStore::open_in_memory().map_err(err("open hierarchy"))?);
    let scope = export_scope();
    seed_scope(&knowledge, &memory, &belief, &hierarchy, &scope)?;

    let exporter = SqlExportImport::new(knowledge, memory)
        .with_belief(belief)
        .with_hierarchy(hierarchy);
    let data = block_on(exporter.export(&scope)).map_err(err("export"))?;

    // The exported payload covers knowledge + taxonomy + memory + belief +
    // hierarchy — the families whose concrete stores expose scope-wide listing
    // methods. Vectors remain deferred (no scope-wide list method).
    let expected = RowCounts {
        memory: 2,
        knowledge_sources: 1,
        knowledge_documents: 1,
        knowledge_chunks: 1,
        knowledge_entities: 2,
        knowledge_relationships: 1,
        concept_schemes: 1,
        concepts: 1,
        beliefs: 1,
        contradictions: 0,
        hierarchy_nodes: 1,
        vectors: 0,
    };
    let actual = RowCounts {
        memory: data.memories.len(),
        knowledge_sources: data.knowledge_sources.len(),
        knowledge_documents: data.knowledge_documents.len(),
        knowledge_chunks: data.knowledge_chunks.len(),
        knowledge_entities: data.knowledge_entities.len(),
        knowledge_relationships: data.knowledge_relationships.len(),
        concept_schemes: data.concept_schemes.len(),
        concepts: data.concepts.len(),
        beliefs: data.beliefs.len(),
        contradictions: 0,
        hierarchy_nodes: data.hierarchy_nodes.len(),
        vectors: data.vectors.len(),
    };
    assert_counts(&expected, &actual)?;

    // Round-trip through the migration service: dry_run → manifest → apply.
    // The validation row counts must match the exported counts (parity).
    let migration = SqlMigrationService::new(384);
    let report = migration
        .dry_run_import(&data)
        .map_err(err("dry_run_import"))?;
    if report.row_counts != expected {
        return Err(CoreError::Adapter {
            adapter: "conformance.export_import".to_string(),
            message: format!(
                "dry_run row counts {:?} do not match exported counts {:?}",
                report.row_counts, expected
            ),
        });
    }
    let manifest = MigrationManifest {
        validation_report: report,
        import_data: data,
    };
    migration
        .apply_import(&manifest)
        .map_err(err("apply_import"))?;
    Ok(())
}

fn assert_counts(expected: &RowCounts, actual: &RowCounts) -> CoreResult<()> {
    if expected != actual {
        return Err(CoreError::Adapter {
            adapter: "conformance.export_import".to_string(),
            message: format!("exported counts {actual:?} do not match expected {expected:?}"),
        });
    }
    Ok(())
}

// ---------- seed helpers --------------------------------------------------

fn export_scope() -> Scope {
    Scope {
        tenant: "tenant-export".to_string(),
        subject: Some("subject-export".to_string()),
        workspace: Some("workspace-export".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("export-fixture-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Export Fixture".to_string()),
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
        source: "export-fixture".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_string()),
    }
}

fn seed_scope(
    knowledge: &Arc<SqlKnowledgeStore>,
    memory: &Arc<SqlMemoryService>,
    belief: &Arc<SqlBeliefStore>,
    hierarchy: &Arc<SqlHierarchyStore>,
    scope: &Scope,
) -> CoreResult<()> {
    block_on(knowledge.put_source(KnowledgeSource {
        id: Id::from("fx-source"),
        kind: SourceKind::Filesystem,
        scope: scope.clone(),
        name: "fixture source".to_string(),
        uri: Some("file:///fixture".to_string()),
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(err("put_source"))?;

    block_on(knowledge.put_document(SourceDocument {
        id: Id::from("fx-doc"),
        source_id: Id::from("fx-source"),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some("docs/intro.md".to_string()),
        title: Some("Fixture Doc".to_string()),
        mime_type: None,
        language: None,
        version: None,
        content_hash: "sha256:fx-doc".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(err("put_document"))?;

    block_on(knowledge.put_chunk(KnowledgeChunk {
        id: Id::from("fx-chunk"),
        document_id: Id::from("fx-doc"),
        source_id: Id::from("fx-source"),
        kind: KnowledgeChunkKind::Paragraph,
        text: "fixture chunk text".to_string(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: "sha256:fx-chunk".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(err("put_chunk"))?;

    for eid in ["fx-entity-a", "fx-entity-b"] {
        block_on(knowledge.put_entity(KnowledgeEntity {
            id: Id::from(eid),
            graph_id: None,
            kind: EntityKind::Function,
            name: format!("entity-{eid}"),
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
        }))
        .map_err(err("put_entity"))?;
    }

    block_on(knowledge.put_relationship(KnowledgeRelationship {
        id: Id::from("fx-rel"),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from("fx-entity-a")),
            kind: Some("function".to_string()),
            name: Some("caller".to_string()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(Id::from("fx-entity-b")),
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
    }))
    .map_err(err("put_relationship"))?;

    for (idx, text) in ["fixture fact one", "fixture fact two"].iter().enumerate() {
        block_on(memory.put_memory(MemoryRecord {
            id: Id::from(format!("fx-mem-{idx}")),
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
        }))
        .map_err(err("put_memory"))?;
    }

    // Taxonomy: one concept scheme + one concept under it.
    block_on(knowledge.put_concept_scheme(ConceptScheme {
        id: Id::from("fx-scheme"),
        uri: "https://example.com/schemes/fx".to_string(),
        name: "fixture scheme".to_string(),
        scope: scope.clone(),
        version: "1.0".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }))
    .map_err(err("put_concept_scheme"))?;

    block_on(knowledge.put_concept(Concept {
        id: Id::from("fx-concept"),
        uri: "https://example.com/concepts/fx".to_string(),
        scheme_id: Id::from("fx-scheme"),
        pref_label: ConceptLabel {
            value: "fixture concept".to_string(),
            language: Some("en".to_string()),
        },
        alt_labels: Vec::new(),
        definition: Some("a concept seeded by the export fixture".to_string()),
        notation: None,
        status: ConceptStatus::Active,
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }))
    .map_err(err("put_concept"))?;

    // Belief: one derived stance record.
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
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(err("put_belief"))?;

    // Hierarchy: one base node.
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
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .map_err(err("put_node"))?;

    Ok(())
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.export_import".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_fixture_passes() {
        if let Err(e) = run_export_import_fixture() {
            panic!("export/import fixture failed: {e}");
        }
    }
}
