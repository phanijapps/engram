//! RFC-0019 T5 regression: MCP `search` routes multi-term / natural-language
//! queries through hybrid recall, returning ranked code-symbol hits.
//!
//! This is the regression guard for the §6.3 defect: `search "reciprocal rank
//! fusion"` returned "No results" because (a) the lexical resolver was
//! chunk-only, so entity-id BM25 hits resolved to `Ok(None)` and were silently
//! dropped by the lane, and (b) the tool fell back to a whole-string
//! `.contains()` loop that missed any non-verbatim-substring query.
//!
//! The test builds the production lexical lane (`lexical_recall_lane`) over a
//! real in-memory `SqlKnowledgeStore` seeded with code-symbol entities, feeds
//! the shared lexical index keyed by *entity id* (exactly as `scan_repo` does),
//! and asserts a multi-term query returns the symbol as a ranked entity hit
//! through `SqlUnifiedRecall`. Verified at the `SqlUnifiedRecall` level — the
//! spec's chosen level (`core/integration` has no sqlite test infra by design),
//! mirroring `tests/associative_recall.rs`.

use std::sync::Arc;

use engram_conformance::SqlUnifiedRecall;
use engram_domain::*;
use engram_integration::UnifiedRecall;
use engram_knowledge::KnowledgeRepository;
use engram_retrieval::RetrievalIndex;
use engram_store_lexical::LexicalIndex;
use engram_store_sqlite::{SqlBeliefStore, SqlKnowledgeStore, SqlMemoryService};
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "t".to_string(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("recall-search-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "recall-search-test".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_string()),
    }
}

fn ent(id: &str, name: &str, kind: EntityKind) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind,
        name: name.to_string(),
        aliases: Vec::new(),
        scope: scope(),
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

fn request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_string(),
        scope: scope(),
        requester: Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
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

/// A multi-term / natural-language query returns the matching code symbol as a
/// ranked entity hit through unified recall. Before T5 this query returned no
/// symbol (the entity-id BM25 hit was dropped by the chunk-only resolver).
#[test]
fn multi_term_query_returns_ranked_symbol_hit_via_recall() {
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));

    // Seed code-symbol entities (mirroring what `scan_repo` ingests).
    let symbols = [
        ("entity-rrf", "ReciprocalRankFusion", EntityKind::Function),
        ("entity-vi", "VectorIndex", EntityKind::Struct),
        ("entity-ms", "MemoryService", EntityKind::Struct),
    ];
    for (id, name, kind) in symbols {
        block_on(knowledge.put_entity(ent(id, name, kind))).expect("put entity");
    }

    // Feed the lexical index keyed by *entity id* with text `"{name} {kind:?}"`
    // — exactly the shape `scan_repo` produces (codegraph.rs `scan_repo`).
    let lexical_index = Arc::new(LexicalIndex::new().expect("lexical index"));
    let entries: Vec<(String, String)> = [
        ("entity-rrf", "ReciprocalRankFusion", EntityKind::Function),
        ("entity-vi", "VectorIndex", EntityKind::Struct),
        ("entity-ms", "MemoryService", EntityKind::Struct),
    ]
    .into_iter()
    .map(|(id, name, kind)| (id.to_string(), format!("{} {:?}", name, kind)))
    .collect();
    lexical_index
        .upsert_batch(&entries)
        .expect("seed lexical index");

    // Production lexical lane (entity-aware resolver) over the seeded store +
    // shared index — the same constructor bootstrap uses.
    let lexical_lane: Arc<dyn RetrievalIndex> =
        engram_integration::sqlite::lexical_recall_lane(knowledge.clone(), lexical_index);
    let recall = SqlUnifiedRecall::new(memory, vec![lexical_lane], beliefs);

    // The §6.3 query: a multi-term natural-language phrase that is NOT a
    // verbatim substring of "ReciprocalRankFusion Function".
    let payload = block_on(recall.recall(request("reciprocal rank fusion"))).expect("recall");

    let rrf_hits: Vec<&RetrievalResult> = payload
        .items
        .iter()
        .filter(|i| i.target_id == "entity-rrf" && i.target_type == RetrievalTargetType::Entity)
        .collect();
    assert_eq!(
        rrf_hits.len(),
        1,
        "the `ReciprocalRankFusion` symbol must appear as a ranked entity hit; \
         items = {:?}",
        payload
            .items
            .iter()
            .map(|i| (
                i.target_type.clone(),
                i.target_id.clone(),
                i.content.clone()
            ))
            .collect::<Vec<_>>()
    );
    // The resolved content carries the symbol name (the entity-aware resolver
    // surfaced it instead of dropping the entity-id hit).
    assert!(
        rrf_hits[0].content.contains("ReciprocalRankFusion"),
        "entity hit content should carry the symbol name: {:?}",
        rrf_hits[0].content
    );
}

/// An unrelated multi-term query does not surface the symbol (no false positive
/// regressions from the entity-resolution branch).
#[test]
fn unrelated_multi_term_query_does_not_match_symbol() {
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    block_on(knowledge.put_entity(ent(
        "entity-rrf",
        "ReciprocalRankFusion",
        EntityKind::Function,
    )))
    .expect("put entity");

    let lexical_index = Arc::new(LexicalIndex::new().expect("lexical index"));
    lexical_index
        .upsert("entity-rrf", "ReciprocalRankFusion Function")
        .expect("seed");

    let lexical_lane: Arc<dyn RetrievalIndex> =
        engram_integration::sqlite::lexical_recall_lane(knowledge.clone(), lexical_index);
    let recall = SqlUnifiedRecall::new(memory, vec![lexical_lane], beliefs);

    let payload = block_on(recall.recall(request("database connection pool"))).expect("recall");
    assert!(
        !payload.items.iter().any(|i| i.target_id == "entity-rrf"),
        "an unrelated multi-term query must not surface ReciprocalRankFusion: {:?}",
        payload.items
    );
}

/// Chunk resolution is unchanged by the entity-aware resolver: a chunk-id hit
/// still resolves to a `Chunk` target (the entity branch is purely additive).
#[test]
fn chunk_resolution_remains_unchanged_with_entity_aware_resolver() {
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("beliefs open"));
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));

    // Seed one chunk whose id is the lexical target_id. The chunk inherits its
    // scope from a parent source (`get_chunk` joins chunk → source), so seed
    // the source first.
    block_on(knowledge.put_source(KnowledgeSource {
        id: SourceId::from("src-1"),
        kind: SourceKind::Filesystem,
        scope: scope(),
        name: "rrf-docs".to_string(),
        uri: None,
        version: None,
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }))
    .expect("put source");
    let chunk = KnowledgeChunk {
        id: ChunkId::from("chunk-doc"),
        document_id: DocumentId::from("doc-1"),
        source_id: SourceId::from("src-1"),
        kind: KnowledgeChunkKind::Paragraph,
        text: "reciprocal rank fusion explainer".to_string(),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: "sha256:rrf-chunk".to_string(),
        provenance: provenance(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };
    block_on(knowledge.put_chunk(chunk)).expect("put chunk");

    let lexical_index = Arc::new(LexicalIndex::new().expect("lexical index"));
    lexical_index
        .upsert("chunk-doc", "reciprocal rank fusion explainer")
        .expect("seed");

    let lexical_lane: Arc<dyn RetrievalIndex> =
        engram_integration::sqlite::lexical_recall_lane(knowledge.clone(), lexical_index);
    let recall = SqlUnifiedRecall::new(memory, vec![lexical_lane], beliefs);

    let payload = block_on(recall.recall(request("reciprocal rank fusion"))).expect("recall");
    let chunk_hit = payload
        .items
        .iter()
        .find(|i| i.target_id == "chunk-doc")
        .expect("chunk hit present");
    assert_eq!(
        chunk_hit.target_type,
        RetrievalTargetType::Chunk,
        "a chunk-id hit must still resolve as a Chunk (entity branch additive only)"
    );
}
