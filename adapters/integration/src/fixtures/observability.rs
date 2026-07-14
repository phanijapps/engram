//! Observability capability fixture (engram-host-sdk brief, S6).
//!
//! Exercises [`SqlObservability`] end-to-end against in-memory knowledge +
//! belief stores: seeds a scope, takes a [`DiagnosticsSnapshot`] through a broad
//! diagnostic scope, and verifies every field is populated with correct,
//! scope-derived counts. This fixture gates the `observability` capability flip
//! in `bootstrap_provider`: the handle attaches and the capability marks
//! `Supported` only when this fixture passes.
//!
//! Verified:
//! - The listable record counts (entities, relationships, sources, chunks,
//!   beliefs) reflect exactly what was seeded.
//! - Degraded types (`documents`, `memories`) report `0` — no list API in v1.
//! - The `CapabilityReport`, `EmbeddingProviderConfig`, schema/adapter versions
//!   are delegated/passed through unchanged.
//! - `slow_query_diagnostics` is `None` in v1.

use std::sync::Arc;

use engram_belief::BeliefRepository;
use engram_domain::*;
use engram_integration::{Observability, RecordCounts};
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use engram_runtime::{CoreError, CoreResult};
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

use engram_integration::sqlite::SqlObservability;

const TENANT: &str = "tenant-obs-fixture";
const SCHEMA_VERSION: &str = "2026.01";
const ADAPTER_VERSION: &str = "0.1.0";

/// Runs the observability capability fixture.
///
/// Seeds knowledge + belief records under a fully-qualified scope, then reads a
/// [`DiagnosticsSnapshot`] through a broad diagnostic scope (same tenant) and
/// verifies every field is populated with correct counts.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if any assertion fails.
pub fn run_observability_fixture() -> CoreResult<()> {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory()?);
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory()?);
    seed_knowledge(&knowledge);
    seed_beliefs(&beliefs);

    let capabilities = sample_report();
    let embedding_config = sample_embedding_config();
    let obs = SqlObservability::new(
        Some(knowledge),
        Some(beliefs),
        diag_scope(),
        capabilities.clone(),
        embedding_config.clone(),
        SCHEMA_VERSION,
        ADAPTER_VERSION,
    );

    let snap = block_on(obs.diagnostics()).map_err(err("diagnostics"))?;

    // ---- Record counts: listable types match the seed exactly ------------
    let counts = &snap.record_counts;
    if counts.entities != 2 {
        return Err(count_mismatch("entities", 2, counts.entities));
    }
    if counts.relationships != 2 {
        return Err(count_mismatch("relationships", 2, counts.relationships));
    }
    if counts.sources != 2 {
        return Err(count_mismatch("sources", 2, counts.sources));
    }
    if counts.chunks != 2 {
        return Err(count_mismatch("chunks", 2, counts.chunks));
    }
    if counts.beliefs != 2 {
        return Err(count_mismatch("beliefs", 2, counts.beliefs));
    }
    // Degraded types: no list API in v1 → 0 (not an error).
    if counts.documents != 0 {
        return Err(count_mismatch("documents (degraded)", 0, counts.documents));
    }
    if counts.memories != 0 {
        return Err(count_mismatch("memories (degraded)", 0, counts.memories));
    }

    // ---- Aggregated fields: delegated / passed through unchanged ---------
    if snap.capabilities != capabilities {
        return Err(err("capabilities")(CoreError::Conflict {
            reason: "capability report was not delegated unchanged".to_string(),
        }));
    }
    if snap.embedding_config != embedding_config {
        return Err(err("embedding_config")(CoreError::Conflict {
            reason: "embedding config was not passed through unchanged".to_string(),
        }));
    }
    if snap.schema_version != SCHEMA_VERSION {
        return Err(err("schema_version")(CoreError::Conflict {
            reason: format!("expected {SCHEMA_VERSION}, got {}", snap.schema_version),
        }));
    }
    if snap.adapter_version != ADAPTER_VERSION {
        return Err(err("adapter_version")(CoreError::Conflict {
            reason: format!("expected {ADAPTER_VERSION}, got {}", snap.adapter_version),
        }));
    }

    // ---- Slow-query diagnostics deferred in v1 ---------------------------
    if snap.slow_query_diagnostics.is_some() {
        return Err(err("slow_query_diagnostics")(CoreError::Conflict {
            reason: "slow_query_diagnostics must be None in v1".to_string(),
        }));
    }

    // All fields populated — the snapshot shape is fully covered.
    let _: &RecordCounts = counts;
    Ok(())
}

fn count_mismatch(field: &str, expected: usize, actual: usize) -> CoreError {
    err("record_counts")(CoreError::Conflict {
        reason: format!("{field}: expected {expected}, got {actual}"),
    })
}

// ---------- seed helpers --------------------------------------------------

fn seed_knowledge(store: &Arc<SqlKnowledgeStore>) {
    let scope = seed_scope();
    for sid in ["source-1", "source-2"] {
        block_on(store.put_source(source(Id::from(sid), scope.clone()))).expect("put_source");
    }
    let graph_id = Id::from("graph-1");
    block_on(store.put_graph(graph(graph_id.clone(), scope.clone()))).expect("put_graph");
    for eid in ["function-a", "function-b"] {
        block_on(store.put_entity(entity(Id::from(eid), graph_id.clone(), scope.clone())))
            .expect("put_entity");
    }
    for (rid, subj, obj) in [
        ("rel-1", "function-a", "function-b"),
        ("rel-2", "function-b", "function-a"),
    ] {
        block_on(store.put_relationship(relationship(
            Id::from(rid),
            Id::from(subj),
            Id::from(obj),
            graph_id.clone(),
            scope.clone(),
        )))
        .expect("put_relationship");
    }
    for (idx, source_id) in [(0, "source-1"), (1, "source-2")] {
        let doc_id = Id::from(format!("document-{idx}"));
        block_on(store.put_document(document(doc_id.clone(), Id::from(source_id))))
            .expect("put_document");
        block_on(store.put_chunk(chunk(
            Id::from(format!("chunk-{idx}")),
            doc_id,
            Id::from(source_id),
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

fn seed_scope() -> Scope {
    Scope {
        tenant: TENANT.to_string(),
        subject: Some("subject-obs".to_string()),
        workspace: Some("workspace-obs".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

/// Broad diagnostic scope: same tenant, all optional fields `None` → sees every
/// record in the tenant.
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

fn document(id: Id, source_id: Id) -> SourceDocument {
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

fn chunk(id: Id, document_id: Id, source_id: Id) -> KnowledgeChunk {
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
        source: "observability-fixture".to_string(),
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

fn sample_report() -> engram_integration::CapabilityReport {
    engram_integration::CapabilityReport::new(CapabilityState::Supported)
}

fn sample_embedding_config() -> engram_integration::EmbeddingProviderConfig {
    engram_integration::EmbeddingProviderConfig {
        provider_type: "test".to_string(),
        model: "test_model".to_string(),
        dimensions: 384,
        prompt_profile: "query".to_string(),
        normalization: None,
    }
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.observability".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observability_fixture_passes() {
        if let Err(e) = run_observability_fixture() {
            panic!("observability fixture failed: {e}");
        }
    }
}
