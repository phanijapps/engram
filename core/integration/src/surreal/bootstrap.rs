//! Surreal backend wiring: construct the embedded SurrealKV-backed cells from
//! configuration and return a wired provider.
//!
//! ADR-0022 (amended 2026-07-16): this module names `Surreal*` / holds the
//! engine adapters by design and is intentionally exempt from the
//! engine-neutrality gate.
//!
//! Connection lifecycle: each cell opens its SurrealKV connection LAZILY on
//! first use (under the consumer's Tokio runtime). The Surreal SDK requires a
//! Tokio reactor, and `bootstrap_surreal` is sync (called from sync
//! `EngramProvider::open`), so the open cannot happen here.

use std::sync::Arc;

use crate::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};
use engram_belief::BeliefRepository;
use engram_domain::{CapabilityState, EmbeddingSpace};
use engram_hierarchy::HierarchyRepository;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::MemoryService;
use engram_retrieval::VectorIndex;
use engram_runtime::CoreResult;
use engram_store_surreal::{
    SurrealBeliefStore, SurrealConnection, SurrealHierarchyStore, SurrealKnowledgeStore,
    SurrealMemoryService, SurrealVectorIndex,
};

/// Bootstraps a fully-wired provider from configuration against the Surreal
/// backend (embedded SurrealKV).
///
/// v1 memory cell wired (plan T4): the memory capability is `Supported` and
/// round-trips write → retrieve → forget through the facade, mirroring the
/// SQLite lifecycle. Other capability cells (knowledge, belief, hierarchy,
/// vectors, consolidation) wire in subsequent increments; until then they
/// report `ProviderUnavailable` (fail-closed, conformance contract).
pub(crate) fn bootstrap_surreal(config: &EngramConfig) -> CoreResult<EngramProvider> {
    let path = config.storage_path.to_string_lossy().to_string();
    // One shared Surreal connection; every Surreal cell (memory now, knowledge/
    // belief/hierarchy/vectors later) clones this Arc.
    let conn = Arc::new(SurrealConnection::new(path));
    // One shared Surreal connection; every Surreal cell clones this Arc.
    let memory: Arc<dyn MemoryService> = Arc::new(SurrealMemoryService::new(conn.clone()));
    let hierarchy: Arc<dyn HierarchyRepository> =
        Arc::new(SurrealHierarchyStore::new(conn.clone()));
    let beliefs: Arc<dyn BeliefRepository> = Arc::new(SurrealBeliefStore::new(conn.clone()));
    // SurrealKnowledgeStore implements all 4 knowledge ports; one Arc, coerced
    // to each trait handle.
    let knowledge_store = Arc::new(SurrealKnowledgeStore::new(conn.clone()));
    let knowledge: Arc<dyn KnowledgeRepository> = knowledge_store.clone();
    let graph: Arc<dyn KnowledgeGraphRepository> = knowledge_store.clone();
    let taxonomy: Arc<dyn TaxonomyRepository> = knowledge_store.clone();
    let ontology: Arc<dyn OntologyRepository> = knowledge_store.clone();
    let embedding_space = EmbeddingSpace::new(
        &config.embedding_provider.provider_type,
        &config.embedding_provider.model,
        config.embedding_provider.dimensions,
        &config.embedding_provider.prompt_profile,
        config.embedding_provider.normalization.clone(),
    );
    let vectors: Arc<dyn VectorIndex> = Arc::new(SurrealVectorIndex::new(conn, embedding_space));
    let report = CapabilityReport::builder()
        .memory(CapabilityState::Supported)
        .hierarchy(CapabilityState::Supported)
        .beliefs(CapabilityState::Supported)
        .knowledge(CapabilityState::Supported)
        .graph(CapabilityState::Supported)
        .taxonomy(CapabilityState::Supported)
        .ontology(CapabilityState::Supported)
        .vectors(CapabilityState::Supported)
        .build();
    Ok(EngramProviderBuilder::new(report)
        .memory(memory)
        .hierarchy(hierarchy)
        .beliefs(beliefs)
        .knowledge(knowledge)
        .graph(graph)
        .taxonomy(taxonomy)
        .ontology(ontology)
        .vectors(vectors)
        .build())
}

#[cfg(test)]
mod tests {
    //! Surreal-cell tests — compile + run only with `--features surreal`.
    use super::*;
    use chrono::{TimeZone, Utc};
    use engram_belief::BeliefQuery;
    use engram_domain::{
        Actor, ActorKind, AllowedUse, Belief, BeliefStatus, BeliefSubject, DeleteMode,
        EmbeddingSpace, ForgetRequest, ForgetStatus, ForgetTargetType, HierarchyNode,
        HierarchyNodeId, HierarchyNodeKind, HierarchyNodeStatus, HierarchyRelation, Id,
        KnowledgeChunk, KnowledgeChunkKind, KnowledgeSource, MemoryContent, MemoryEventKind,
        MemoryKind, MemoryStatus, Policy, Provenance, Requester, Retention, RetrievalRequest,
        Scope, Sensitivity, SourceDocument, SourceDocumentKind, SourceKind, Visibility,
        WriteMemoryRequest, types::ScopeMappingStrategy,
    };
    use tempfile::TempDir;

    fn test_config(dir: &TempDir) -> EngramConfig {
        EngramConfig::new(
            dir.path().join("surreal"),
            dir.path(),
            ScopeMappingStrategy::Strict,
            crate::EmbeddingProviderConfig {
                provider_type: "fastembed".to_string(),
                model: "m".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            crate::MigrationMode::DryRun,
            crate::CapabilityPolicy::FailClosed,
        )
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
            id: Id::from("surreal-agent"),
            kind: ActorKind::Agent,
            display_name: Some("Surreal".to_owned()),
            metadata: None,
        }
    }

    fn requester() -> Requester {
        Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: vec!["memory.write".to_owned(), "memory.retrieve".to_owned()],
            on_behalf_of: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "surreal-test".to_owned(),
            actor: actor(),
            observed_at: Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("manual".to_owned()),
        }
    }

    fn policy() -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Medium),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        }
    }

    fn write_request(tenant: &str) -> WriteMemoryRequest {
        WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: "surreal round-trip memory".to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope: scope(tenant),
            requester: requester(),
            provenance: provenance(),
            policy: policy(),
            links: Vec::new(),
            idempotency_key: None,
        }
    }

    fn retrieve_request(tenant: &str) -> RetrievalRequest {
        RetrievalRequest {
            query: "surreal".to_owned(),
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

    fn forget_request(id: &str, tenant: &str) -> ForgetRequest {
        ForgetRequest {
            target_type: ForgetTargetType::Memory,
            target_id: id.to_owned(),
            scope: scope(tenant),
            requester: requester(),
            mode: DeleteMode::Tombstone,
            reason: None,
        }
    }

    #[test]
    fn bootstrap_surreal_constructs_provider_with_memory_supported() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        assert!(
            provider.capabilities().memory.is_supported(),
            "the Surreal memory cell (T4) must mark memory Supported"
        );
        assert!(
            provider.memory().is_some(),
            "the memory handle must be wired"
        );
    }

    /// T4 proof: the Surreal memory cell round-trips write → retrieve → forget
    /// through the facade's memory handle, with scope isolation — the same
    /// lifecycle the SQLite fixtures + S7 stub exercise, here against SurrealKV.
    #[tokio::test]
    async fn surreal_memory_round_trips_write_retrieve_forget() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        let memory = provider.memory().expect("memory handle wired");

        // write
        let stored = memory
            .write_memory(write_request("tenant-a"))
            .await
            .expect("write_memory against Surreal");
        let memory_id = stored.record.id.to_string();
        assert_eq!(stored.record.status, MemoryStatus::Active);
        assert_eq!(stored.event.kind, MemoryEventKind::Written);

        // retrieve — tenant-a sees it
        let visible = memory
            .retrieve(retrieve_request("tenant-a"))
            .await
            .expect("retrieve tenant-a");
        assert!(
            visible.items.iter().any(|r| r.target_id == memory_id),
            "tenant-a must see its Surreal-stored memory"
        );

        // scope isolation — tenant-b must not
        let hidden = memory
            .retrieve(retrieve_request("tenant-b"))
            .await
            .expect("retrieve tenant-b");
        assert!(
            !hidden.items.iter().any(|r| r.target_id == memory_id),
            "Surreal backend must not leak memories across tenants"
        );

        // forget (tombstone)
        let forgotten = memory
            .forget(forget_request(&memory_id, "tenant-a"))
            .await
            .expect("forget against Surreal");
        assert_eq!(forgotten.status, ForgetStatus::Tombstoned);

        // after tombstone, not visible
        let after = memory
            .retrieve(retrieve_request("tenant-a"))
            .await
            .expect("retrieve after forget");
        assert!(
            !after.items.iter().any(|r| r.target_id == memory_id),
            "tombstoned memory must not appear in Surreal retrieval"
        );
    }

    fn h_node(id: &str, layer: u32, parent: Option<&str>) -> HierarchyNode {
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
            provenance: provenance(),
            created_at: Utc::now(),
            updated_at: None,
            metadata: None,
        }
    }

    fn h_relation(id: &str, src: &str, tgt: &str) -> HierarchyRelation {
        HierarchyRelation {
            id: id.to_owned(),
            scope: scope("tenant-a"),
            source_id: HierarchyNodeId::from(src),
            target_id: HierarchyNodeId::from(tgt),
            predicate: "parent_of".to_owned(),
            layer: None,
            strength: None,
            is_inter_cluster: None,
            evidence: Vec::new(),
            provenance: provenance(),
            created_at: Utc::now(),
        }
    }

    /// Surreal hierarchy cell: build a 3-node parent chain (a→b→c) and walk the
    /// parent path from the leaf — same fixture the SQLite adapter exercises.
    #[tokio::test]
    async fn surreal_hierarchy_walks_parent_chain() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        let hierarchy = provider.hierarchy().expect("hierarchy handle wired");
        let s = scope("tenant-a");

        hierarchy
            .put_node(h_node("a", 0, None))
            .await
            .expect("put_node a");
        hierarchy
            .put_node(h_node("b", 1, Some("a")))
            .await
            .expect("put_node b");
        hierarchy
            .put_node(h_node("c", 2, Some("b")))
            .await
            .expect("put_node c");
        hierarchy
            .put_relation(h_relation("r1", "a", "b"))
            .await
            .expect("put_relation r1");
        hierarchy
            .put_relation(h_relation("r2", "b", "c"))
            .await
            .expect("put_relation r2");

        let path = hierarchy
            .path_for(&["c".to_string()], &s, None)
            .await
            .expect("path_for");
        assert_eq!(path.nodes.len(), 3, "3-node parent chain a->b->c");
        assert_eq!(path.nodes[0].id, HierarchyNodeId::from("a"), "root-first");
        assert_eq!(path.nodes[2].id, HierarchyNodeId::from("c"), "leaf last");
    }

    fn ts(seconds: i64) -> chrono::DateTime<chrono::Utc> {
        Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
    }

    fn s_belief(
        id: &str,
        key: &str,
        content: &str,
        confidence: f32,
        valid_from: Option<chrono::DateTime<chrono::Utc>>,
        valid_until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Belief {
        let mut b = Belief {
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
            valid_from,
            valid_until,
            superseded_by: None,
            stale: None,
            synthesizer: None,
            reasoning: None,
            embedding_refs: Vec::new(),
            policy: policy(),
            provenance: provenance(),
            created_at: Utc::now(),
            updated_at: None,
            metadata: None,
        };
        b.created_at = valid_from.unwrap_or_else(Utc::now);
        b
    }

    /// Surreal belief cell: valid-time lookup returns the belief live at that
    /// time; record-time history is rejected — same fixture as the SQLite adapter.
    #[tokio::test]
    async fn surreal_belief_valid_time_lookup_and_record_time_rejected() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        let beliefs = provider.beliefs().expect("belief handle wired");
        let s = scope("tenant-a");

        let old = s_belief("b-old", "svc-a", "old", 0.7, Some(ts(10)), Some(ts(20)));
        let current = s_belief("b-cur", "svc-a", "cur", 0.9, Some(ts(20)), None);
        beliefs.put_belief(old).await.expect("put old");
        beliefs.put_belief(current).await.expect("put current");

        // valid-time during the old window → old belief
        let at15 = beliefs
            .get_belief(BeliefQuery::live_subject(s.clone(), "svc-a", ts(15)))
            .await
            .expect("get_belief t=15")
            .expect("belief live at t=15");
        assert_eq!(at15.id, Id::from("b-old"));

        // valid-time after switchover → current belief
        let at40 = beliefs
            .get_belief(BeliefQuery::live_subject(s.clone(), "svc-a", ts(40)))
            .await
            .expect("get_belief t=40")
            .expect("belief live at t=40");
        assert_eq!(at40.id, Id::from("b-cur"));

        // record-time history is unsupported → rejected
        let mut record_query = BeliefQuery::live_subject(s, "svc-a", ts(40));
        record_query.recorded_at = Some(ts(25));
        assert!(
            beliefs.get_belief(record_query).await.is_err(),
            "record-time history query must be rejected"
        );
    }

    fn k_source(id: &str) -> KnowledgeSource {
        KnowledgeSource {
            id: Id::from(id),
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

    fn k_document(id: &str, source_id: &str) -> SourceDocument {
        SourceDocument {
            id: Id::from(id),
            source_id: Id::from(source_id),
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

    fn k_chunk(id: &str, document_id: &str, source_id: &str) -> KnowledgeChunk {
        KnowledgeChunk {
            id: Id::from(id),
            document_id: Id::from(document_id),
            source_id: Id::from(source_id),
            kind: KnowledgeChunkKind::Paragraph,
            text: "surreal chunk".to_owned(),
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

    /// Surreal knowledge cell: source → document → chunk round-trip with scope
    /// isolation (chunk visibility inherits from its owning source).
    #[tokio::test]
    async fn surreal_knowledge_chunk_round_trip_with_scope_isolation() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        let knowledge = provider.knowledge().expect("knowledge handle wired");

        knowledge
            .put_source(k_source("s1"))
            .await
            .expect("put_source");
        knowledge
            .put_document(k_document("d1", "s1"))
            .await
            .expect("put_document");
        knowledge
            .put_chunk(k_chunk("c1", "d1", "s1"))
            .await
            .expect("put_chunk");

        // tenant-a owns the source → chunk visible
        let visible = knowledge
            .get_chunk(&Id::from("c1"), &scope("tenant-a"))
            .await
            .expect("get_chunk tenant-a");
        assert!(visible.is_some(), "tenant-a must see its chunk");

        // tenant-b does not own the source → chunk hidden (scope inheritance)
        let hidden = knowledge
            .get_chunk(&Id::from("c1"), &scope("tenant-b"))
            .await
            .expect("get_chunk tenant-b");
        assert!(hidden.is_none(), "tenant-b must not see tenant-a's chunk");
    }

    /// Surreal vector cell: insert a vector, search returns the target.
    #[tokio::test]
    async fn surreal_vector_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = test_config(&dir);
        // Build the embedding space the same way bootstrap_surreal does.
        let space = EmbeddingSpace::new(
            &cfg.embedding_provider.provider_type,
            &cfg.embedding_provider.model,
            cfg.embedding_provider.dimensions,
            &cfg.embedding_provider.prompt_profile,
            cfg.embedding_provider.normalization.clone(),
        );
        let provider = bootstrap_surreal(&cfg).expect("surreal bootstrap");
        let vectors = provider.vectors().expect("vectors handle wired");
        let target = Id::from("chunk-1");
        let dims = cfg.embedding_provider.dimensions as usize;
        vectors
            .insert(&target, &space, vec![0.1; dims])
            .await
            .expect("insert");
        let hits = vectors
            .search(&space, vec![0.1; dims], 1)
            .await
            .expect("search");
        assert!(
            hits.iter().any(|(id, _)| *id == target),
            "search returns the inserted target"
        );
    }
}
