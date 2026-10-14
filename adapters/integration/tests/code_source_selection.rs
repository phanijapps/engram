//! Real candidate feeds and stale secondary projections obey source eligibility.
use engram_domain::*;
use engram_integration::KnowledgeQuery;
use engram_knowledge::{CodeSourceRepository, CodeSourceWrite, KnowledgeRepository};
use engram_retrieval::{InMemoryGraphCache, RetrievalIndex};
use engram_store_lexical::LexicalIndex;
use engram_store_sqlite::{GraphRetrievalIndex, SqlKnowledgeStore};
use futures::executor::block_on;
use serde_json::json;
use std::sync::Arc;

fn seeded() -> (
    Arc<SqlKnowledgeStore>,
    CodeSourceGeneration,
    RetrievalRequest,
) {
    seeded_with_policy(
        json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}),
    )
}
fn seeded_with_policy(
    policy: serde_json::Value,
) -> (
    Arc<SqlKnowledgeStore>,
    CodeSourceGeneration,
    RetrievalRequest,
) {
    let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let provenance = json!({"source":"test","actor":{"id":"owner","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"});
    let hash = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let file = json!({"scope":{"tenant":"t"},"repository":"repo","path":"a.ts"});
    let m:CodeSourceGeneration=serde_json::from_value(json!({"file":file,"generation":"v1","attemptId":"first","sourceHash":hash,"fingerprint":"code-v1","state":"updating","byteLength":5,"documentId":"doc","snapshotIds":["snapshot"],"records":[{"kind":"source","id":"source"},{"kind":"document","id":"doc"},{"kind":"chunk","id":"snapshot"},{"kind":"chunk","id":"unit"},{"kind":"entity","id":"one"},{"kind":"entity","id":"two"},{"kind":"relationship","id":"call"}],"policy":policy,"provenance":provenance})).unwrap();
    block_on(store.begin_code_source(m.clone(), None)).unwrap();
    block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
    block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Source(serde_json::from_value(json!({"id":"source","kind":"filesystem","scope":m.file.scope,"name":"test","metadata":{"stableSourceKey":"repo"},"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Document(serde_json::from_value(json!({"id":"doc","sourceId":"source","kind":"code","path":"a.ts","contentHash":hash,"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    for (id, role) in [("snapshot", "source_snapshot"), ("unit", "symbol")] {
        block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Chunk(serde_json::from_value(json!({"id":id,"sourceId":"source","documentId":"doc","kind":"code_block","documentKind":"code","text":"hello","contentHash":hash,"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z","code":{"role":role,"file":file,"generation":"v1","sourceHash":hash,"fingerprint":"code-v1","ranges":[{"start":0,"end":5}],"complete":true,"parseQuality":"clean"}})).unwrap()))).unwrap();
    }
    for (id, name) in [("one", "getUser"), ("two", "deleteUser")] {
        block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Entity(serde_json::from_value(json!({"id":id,"kind":"function","name":name,"scope":m.file.scope,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    }
    block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Relationship(serde_json::from_value(json!({"id":"call","subject":{"id":"one"},"predicate":"calls","object":{"id":"two"},"scope":m.file.scope,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    let mut active = m.clone();
    active.state = CodeSourceState::Active;
    block_on(store.activate_code_source(active)).unwrap();
    let request=serde_json::from_value(json!({"scope":m.file.scope,"query":"getUser","requester":{"actor":{"id":"owner","kind":"agent"}}})).unwrap();
    (store, m, request)
}

#[test]
fn snapshot_hash_twin_and_stale_lexical_hit_are_never_candidates() {
    let (store, m, mut request) = seeded();
    let refs = block_on(KnowledgeQuery::list_chunk_refs(
        store.as_ref(),
        &m.file.scope,
    ))
    .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id.as_str(), "unit");
    assert!(refs[0].semantic_eligible);
    assert!(refs[0].code.is_some());
    assert!(
        !refs[0].model_eligible,
        "semantic eligibility is not model egress authorization"
    );
    assert!(
        block_on(store.get_chunk(&Id::from("snapshot"), &m.file.scope))
            .unwrap()
            .is_none()
    );
    let index = Arc::new(LexicalIndex::new().unwrap());
    index.upsert("snapshot", "hello").unwrap();
    index.upsert("unit", "hello").unwrap();
    let lane = engram_integration::sqlite::lexical_recall_lane(store.clone(), index);
    request.query = "hello".into();
    let hits = block_on(lane.retrieve_candidates(&request)).unwrap();
    assert!(hits.iter().any(|r| r.target_id == "unit"));
    assert!(hits.iter().all(|r| r.target_id != "snapshot"));
    let mut next = m.clone();
    next.attempt_id = "next".into();
    block_on(store.begin_code_source(next, Some("first"))).unwrap();
    assert!(
        block_on(lane.retrieve_candidates(&request))
            .unwrap()
            .is_empty()
    );
    assert!(
        block_on(KnowledgeQuery::list_chunk_refs(
            store.as_ref(),
            &m.file.scope
        ))
        .unwrap()
        .is_empty()
    );
}

#[test]
fn all_cached_graph_lanes_drop_an_updating_file() {
    let (store, m, request) = seeded();
    let cache = Arc::new(InMemoryGraphCache::new());
    let lanes: Vec<Arc<dyn RetrievalIndex>> = vec![
        engram_integration::sqlite::associative_recall_lane(store.clone(), Some(cache.clone())),
        engram_integration::sqlite::community_summary_recall_lane(
            store.clone(),
            Some(cache.clone()),
        ),
        Arc::new(GraphRetrievalIndex::with_cache(store.clone(), 20, cache)),
    ];
    for lane in &lanes {
        assert!(
            !block_on(lane.retrieve_candidates(&request))
                .unwrap()
                .is_empty()
        );
    }
    let mut next = m.clone();
    next.attempt_id = "next".into();
    block_on(store.begin_code_source(next, Some("first"))).unwrap();
    for lane in &lanes {
        assert!(
            block_on(lane.retrieve_candidates(&request))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn a_reused_active_entity_id_does_not_make_its_old_lexical_text_current() {
    let (store, m, request) = seeded();
    let index = Arc::new(LexicalIndex::new().unwrap());
    index.upsert("one", "getUser Function").unwrap();
    let lane = engram_integration::sqlite::lexical_recall_lane(store.clone(), index);
    assert!(
        !block_on(lane.retrieve_candidates(&request))
            .unwrap()
            .is_empty()
    );
    let mut next = m.clone();
    next.attempt_id = "next".into();
    block_on(store.begin_code_source(next.clone(), Some("first"))).unwrap();
    block_on(store.stage_code_records(&next.file, &next.attempt_id, &next.records)).unwrap();
    for (owner, record) in block_on(store.prior_code_records(&next.file, &next.attempt_id)).unwrap()
    {
        block_on(store.remove_code_record(&next.file, &next.attempt_id, &owner, &record)).unwrap();
    }
    let mut entity:KnowledgeEntity=serde_json::from_value(json!({"id":"one","kind":"function","name":"newUserBehavior","scope":m.file.scope,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    entity.graph_id = None;
    block_on(store.write_code_record(
        &next.file,
        &next.attempt_id,
        CodeSourceWrite::Entity(entity),
    ))
    .unwrap();
    next.state = CodeSourceState::Active;
    block_on(store.activate_code_source(next)).unwrap();
    assert!(
        block_on(store.get_entity(&Id::from("one"), &m.file.scope))
            .unwrap()
            .is_some()
    );
    assert!(
        block_on(lane.retrieve_candidates(&request))
            .unwrap()
            .is_empty(),
        "source eligibility alone must not validate old projection text"
    );
}

#[test]
fn source_policy_and_exact_scope_are_enforced_after_cache_hits_in_every_lane() {
    for policy in [
        json!({"visibility":"private","retention":"durable","allowedUses":["retrieval"]}),
        json!({"visibility":"workspace","retention":"durable","allowedUses":[]}),
        json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"],"expiresAt":"2020-01-01T00:00:00Z"}),
    ] {
        let (store, m, owner) = seeded_with_policy(policy.clone());
        let cache = Arc::new(InMemoryGraphCache::new());
        let lexical = Arc::new(LexicalIndex::new().unwrap());
        lexical.upsert("one", "getUser Function").unwrap();
        let lanes: Vec<Arc<dyn RetrievalIndex>> = vec![
            Arc::new(GraphRetrievalIndex::with_cache(
                store.clone(),
                20,
                cache.clone(),
            )),
            engram_integration::sqlite::associative_recall_lane(store.clone(), Some(cache.clone())),
            engram_integration::sqlite::community_summary_recall_lane(store.clone(), Some(cache)),
            engram_integration::sqlite::lexical_recall_lane(store.clone(), lexical),
        ];
        for lane in &lanes {
            let _ = block_on(lane.retrieve_candidates(&owner)).unwrap();
        }
        let mut stranger = owner.clone();
        stranger.requester.actor.id = Id::from("stranger");
        for lane in &lanes {
            assert!(
                block_on(lane.retrieve_candidates(&stranger))
                    .unwrap()
                    .is_empty(),
                "source policy must apply to every lane: {:?}",
                m.policy
            );
        }
        let mut wrong_scope = owner.clone();
        wrong_scope.scope.session = Some("other".into());
        for lane in &lanes {
            assert!(
                block_on(lane.retrieve_candidates(&wrong_scope))
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn expiry_after_cache_warmup_is_rechecked_by_every_lane() {
    let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(2)).to_rfc3339();
    let (store, _m, request) = seeded_with_policy(json!({
        "visibility":"workspace",
        "retention":"durable",
        "allowedUses":["retrieval"],
        "expiresAt":expires_at,
    }));
    let cache = Arc::new(InMemoryGraphCache::new());
    let lexical = Arc::new(LexicalIndex::new().unwrap());
    lexical.upsert("one", "getUser Function").unwrap();
    let lanes: Vec<Arc<dyn RetrievalIndex>> = vec![
        Arc::new(GraphRetrievalIndex::with_cache(
            store.clone(),
            20,
            cache.clone(),
        )),
        engram_integration::sqlite::associative_recall_lane(store.clone(), Some(cache.clone())),
        engram_integration::sqlite::community_summary_recall_lane(store.clone(), Some(cache)),
        engram_integration::sqlite::lexical_recall_lane(store, lexical),
    ];

    for lane in &lanes {
        assert!(
            !block_on(lane.retrieve_candidates(&request))
                .unwrap()
                .is_empty(),
            "each lane should have a candidate before source expiry"
        );
    }
    std::thread::sleep(std::time::Duration::from_millis(2_100));
    for lane in &lanes {
        assert!(
            block_on(lane.retrieve_candidates(&request))
                .unwrap()
                .is_empty(),
            "a warm projection must not outlive its source policy"
        );
    }
}

#[test]
fn shared_record_uses_one_whole_authorized_owner_policy() {
    let mut access = engram_retrieval::RecordAccess::default();
    access.restrict(RetrievalTargetType::Entity, "shared");
    let workspace_owner: Policy = serde_json::from_value(json!({
        "visibility":"workspace",
        "retention":"durable",
        "allowedUses":["retrieval","training_export"]
    }))
    .unwrap();
    let authorized_owner: Policy = serde_json::from_value(json!({
        "visibility":"private",
        "retention":"ephemeral",
        "allowedUses":["retrieval"],
        "expiresAt":"2099-01-01T00:00:00Z"
    }))
    .unwrap();
    access.permit(RetrievalTargetType::Entity, "shared", workspace_owner);
    access.permit(
        RetrievalTargetType::Entity,
        "shared",
        authorized_owner.clone(),
    );
    let selected = access
        .policy(RetrievalTargetType::Entity, "shared")
        .expect("shared entity should have an authorized owner");
    assert_eq!(selected, &authorized_owner);
    assert_eq!(selected.visibility, Visibility::Private);
    assert_eq!(selected.retention, Retention::Ephemeral);
    assert_eq!(selected.expires_at, authorized_owner.expires_at);
    assert_eq!(selected.allowed_uses, authorized_owner.allowed_uses);
}

#[test]
fn private_unavailable_source_metadata_is_not_disclosed() {
    let policy = json!({
        "visibility":"private",
        "retention":"durable",
        "allowedUses":["retrieval"]
    });
    let (store, mut manifest, _) = seeded_with_policy(policy);
    manifest.attempt_id = "refreshing".into();
    block_on(store.begin_code_source(manifest.clone(), Some("first"))).unwrap();

    let request = CodeSourceReadRequest {
        file: manifest.file.clone(),
        requester: serde_json::from_value(json!({"actor":{"id":"stranger","kind":"agent"}}))
            .unwrap(),
        generation: None,
        max_bytes: 1024,
    };
    assert!(block_on(store.read_code_source(request, &manifest.file.scope)).is_err());
}
