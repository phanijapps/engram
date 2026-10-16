//! Managed document authority clamps cached retrieval projections.
use engram_domain::*;
use engram_knowledge::{DocumentHeadRepository, DocumentHeadWrite, KnowledgeGraphRepository};
use engram_retrieval::{InMemoryGraphCache, RetrievalIndex};
use engram_store_lexical::LexicalIndex;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;
use std::sync::Arc;

fn stage(
    store: &SqlKnowledgeStore,
    attempt: &str,
    expected: Option<&str>,
    text: &str,
    private: bool,
) -> DocumentHead {
    let h: DocumentHead = serde_json::from_value(json!({"key":{"scope":{"tenant":"t","workspace":"w"},"repository":"repo","sourceKey":"guide.md"},"generation":attempt,"attemptId":attempt,"state":"updating","sourceHash":attempt,"documentId":"doc","chunkIds":[format!("chunk-{attempt}")],"policy":{"visibility":if private {"private"} else {"workspace"},"retention":"durable","allowedUses":["retrieval"]},"provenance":{"source":"test","actor":{"id":"owner","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"}})).unwrap();
    block_on(store.begin_document_head(h.clone(), expected)).unwrap();
    let source = serde_json::from_value(json!({"id":"source","kind":"filesystem","scope":h.key.scope,"name":"repo","metadata":{"stableSourceKey":"repo"},"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    let doc = serde_json::from_value(json!({"id":"doc","sourceId":"source","kind":"markdown","path":"guide.md","contentHash":attempt,"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    let chunk = serde_json::from_value(json!({"id":h.chunk_ids[0],"sourceId":"source","documentId":"doc","kind":"paragraph","documentKind":"markdown","text":text,"contentHash":attempt,"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    for payload in [
        DocumentHeadWrite::Source(source),
        DocumentHeadWrite::Document(doc),
        DocumentHeadWrite::Chunk(Box::new(chunk)),
    ] {
        block_on(store.write_document_record(&h.key, attempt, payload)).unwrap();
    }
    h
}
fn activate(store: &SqlKnowledgeStore, mut h: DocumentHead) {
    h.state = CodeSourceState::Active;
    block_on(store.activate_document_head(h)).unwrap();
}

#[test]
fn stale_text_updating_attempt_private_policy_and_scope_drop_cached_hits() {
    let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    activate(&store, stage(&store, "a", None, "original body", false));
    let index = Arc::new(LexicalIndex::new().unwrap());
    index.upsert("chunk-a", "original body").unwrap();
    let lane = engram_integration::sqlite::lexical_recall_lane(store.clone(), index.clone());
    let request: RetrievalRequest = serde_json::from_value(json!({"scope":{"tenant":"t","workspace":"w"},"query":"original","requester":{"actor":{"id":"owner","kind":"agent"}}})).unwrap();
    assert_eq!(
        block_on(lane.retrieve_candidates(&request)).unwrap().len(),
        1
    );
    let b = stage(&store, "b", Some("a"), "replacement body", false);
    assert!(
        block_on(lane.retrieve_candidates(&request))
            .unwrap()
            .is_empty()
    );
    activate(&store, b);
    index.upsert("chunk-b", "original body").unwrap();
    assert!(
        block_on(lane.retrieve_candidates(&request))
            .unwrap()
            .is_empty(),
        "an active reused ID cannot bless stale indexed text"
    );
    activate(&store, stage(&store, "c", Some("b"), "original body", true));
    index.upsert("chunk-c", "original body").unwrap();
    assert_eq!(
        block_on(lane.retrieve_candidates(&request)).unwrap().len(),
        1
    );
    let mut outsider = request.clone();
    outsider.requester.actor.id = Id::from("outsider");
    assert!(
        block_on(lane.retrieve_candidates(&outsider))
            .unwrap()
            .is_empty()
    );
    let mut wrong_scope = request;
    wrong_scope.scope.session = Some("other".into());
    assert!(
        block_on(lane.retrieve_candidates(&wrong_scope))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn warmed_generic_graph_projections_obey_document_head_policy_and_state() {
    let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let h = stage(&store, "a", None, "original body", true);
    let graph = serde_json::from_value(json!({"id":"graph","name":"document graph","scope":h.key.scope,"metadata":{"documentId":"doc","stableSourceKey":"repo"},"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    block_on(store.write_document_record(&h.key, &h.attempt_id, DocumentHeadWrite::Graph(graph)))
        .unwrap();
    for id in ["original", "destination"] {
        let entity = serde_json::from_value(json!({"id":id,"name":id,"kind":"function","graphId":"graph","scope":h.key.scope,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
        block_on(store.write_document_record(
            &h.key,
            &h.attempt_id,
            DocumentHeadWrite::Entity(entity),
        ))
        .unwrap();
    }
    let rel = serde_json::from_value(json!({"id":"edge","graphId":"graph","subject":{"id":"original"},"predicate":"calls","object":{"id":"destination"},"scope":h.key.scope,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    block_on(store.write_document_record(
        &h.key,
        &h.attempt_id,
        DocumentHeadWrite::Relationship(rel),
    ))
    .unwrap();
    activate(&store, h.clone());
    let cache = Arc::new(InMemoryGraphCache::new());
    let lanes: Vec<Arc<dyn RetrievalIndex>> = vec![
        Arc::new(engram_store_sqlite::GraphRetrievalIndex::with_cache(
            store.clone(),
            20,
            cache.clone(),
        )),
        engram_integration::sqlite::associative_recall_lane(store.clone(), Some(cache.clone())),
        engram_integration::sqlite::community_summary_recall_lane(store.clone(), Some(cache)),
    ];
    let owner: RetrievalRequest = serde_json::from_value(json!({"scope":h.key.scope,"query":"original","requester":{"actor":{"id":"owner","kind":"agent"}}})).unwrap();
    for lane in &lanes {
        assert!(
            !block_on(lane.retrieve_candidates(&owner))
                .unwrap()
                .is_empty()
        );
    }
    let mut outsider = owner.clone();
    outsider.requester.actor.id = Id::from("outsider");
    for lane in &lanes {
        assert!(
            block_on(lane.retrieve_candidates(&outsider))
                .unwrap()
                .is_empty()
        );
    }
    let mut forged: KnowledgeGraph = serde_json::from_value(json!({"id":"graph","name":"forged","scope":h.key.scope,"policy":h.policy,"provenance":h.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    assert!(
        block_on(store.put_graph(forged.clone())).is_err(),
        "metadata removal must not remove managed ownership"
    );
    forged.metadata = Some(
        json!({"documentId":"other"})
            .as_object()
            .unwrap()
            .clone()
            .into_iter()
            .collect(),
    );
    assert!(block_on(store.put_graph(forged)).is_err());
    store.lock().unwrap().execute("UPDATE knowledge_graphs_records SET record_json=json_remove(record_json,'$.metadata.documentId') WHERE id='graph'", []).unwrap();
    for lane in &lanes {
        assert!(
            block_on(lane.retrieve_candidates(&owner))
                .unwrap()
                .is_empty()
        );
    }
    let access = store.source_record_access(&owner).unwrap();
    assert!(
        !access.allows(RetrievalTargetType::Entity, "original"),
        "raw metadata mutation cannot erase ownership restrictions"
    );
    let _ = stage(&store, "b", Some("a"), "replacement body", true);
    for lane in &lanes {
        assert!(
            block_on(lane.retrieve_candidates(&owner))
                .unwrap()
                .is_empty()
        );
    }
}
