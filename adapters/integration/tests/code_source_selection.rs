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
    seeded_with_role(policy, "symbol")
}
fn seeded_with_role(
    policy: serde_json::Value,
    role: &str,
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
    for (id, role) in [("snapshot", "source_snapshot"), ("unit", role)] {
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

mod local_embedding_backfill {
    use super::*;
    use engram_integration::{
        CapabilityReport, EmbeddingProvider, EngramProvider, EngramProviderBuilder,
    };
    use engram_retrieval::VectorIndex;
    use engram_runtime::{CoreError, CoreResult};
    use engram_store_sqlite::SqliteVectorIndex;
    use futures::FutureExt;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };

    struct RecordingModel {
        local: bool,
        inputs: Mutex<Vec<String>>,
        batch_sizes: Mutex<Vec<usize>>,
        fail: AtomicBool,
        before_return: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    }
    impl RecordingModel {
        fn new(local: bool) -> Self {
            Self {
                local,
                inputs: Mutex::new(vec![]),
                batch_sizes: Mutex::new(vec![]),
                fail: AtomicBool::new(false),
                before_return: Mutex::new(None),
            }
        }
    }
    impl EmbeddingProvider for RecordingModel {
        fn is_local(&self) -> bool {
            self.local
        }
        fn provider_id(&self) -> &str {
            "recording"
        }
        fn model_id(&self) -> &str {
            "deterministic"
        }
        fn dimensions(&self) -> u32 {
            3
        }
        fn embedding_space(&self) -> EmbeddingSpace {
            EmbeddingSpace::new("recording", "deterministic", 3, "passage", None::<String>)
        }
        fn embed_query(&self, _query: &str) -> CoreResult<Vec<f32>> {
            Ok(vec![1., 0., 0.])
        }
        fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
            self.inputs.lock().unwrap().push(text.to_owned());
            if self.fail.load(Ordering::SeqCst) {
                return Err(CoreError::Adapter {
                    adapter: "recording_model".into(),
                    message: "inference failed".into(),
                });
            }
            if let Some(callback) = self.before_return.lock().unwrap().take() {
                callback();
            }
            Ok(vec![1., 0., 0.])
        }
        fn embed_batch(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
            self.batch_sizes.lock().unwrap().push(texts.len());
            texts.iter().map(|text| self.embed_passage(text)).collect()
        }
    }
    fn provider(
        store: Arc<SqlKnowledgeStore>,
        model: Arc<RecordingModel>,
    ) -> (EngramProvider, Arc<SqliteVectorIndex>) {
        let index = Arc::new(
            SqliteVectorIndex::open_in_memory(3)
                .unwrap()
                .with_embedding_space(model.embedding_space()),
        );
        let provider =
            EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
                .knowledge(store.clone())
                .knowledge_query(store)
                .embedding_provider(model)
                .vectors(index.clone())
                .build();
        (provider, index)
    }
    fn request(request: &RetrievalRequest) -> EmbeddingBackfillRequest {
        EmbeddingBackfillRequest {
            scope: request.scope.clone(),
            requester: request.requester.clone(),
            limit: 10,
            source_filter: None,
        }
    }

    #[test]
    fn local_backfill_embeds_code_and_never_snapshot_hash_twin_and_repeat_is_idle() {
        let (store, _, retrieval) = seeded();
        let model = Arc::new(RecordingModel::new(true));
        let (provider, index) = provider(store, model.clone());
        let first = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
        assert_eq!(first.embedded, 1);
        assert_eq!(first.new_embeddings, 1);
        assert_eq!(first.code_embedded, 1);
        assert_eq!(first.remaining, 0);
        assert_eq!(*model.inputs.lock().unwrap(), vec!["hello"]);
        assert_eq!(
            block_on(index.embedded_ids()).unwrap(),
            [Id::from("unit")].into_iter().collect()
        );
        let second = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
        assert_eq!(second.embedded, 0);
        assert_eq!(second.remaining, 0);
        assert_eq!(model.inputs.lock().unwrap().len(), 1);
    }

    #[test]
    fn external_backfill_receives_no_code_even_when_semantically_eligible() {
        let (store, _, retrieval) = seeded();
        let model = Arc::new(RecordingModel::new(false));
        let (provider, index) = provider(store, model.clone());
        let report = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
        assert_eq!(report.embedded, 0);
        assert!(model.inputs.lock().unwrap().is_empty());
        assert!(block_on(index.embedded_ids()).unwrap().is_empty());
    }

    #[test]
    fn local_backfill_rejects_denied_expired_private_and_wrong_scope_before_inference() {
        for policy in [
            json!({"visibility":"workspace","retention":"durable","allowedUses":[]}),
            json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"],"expiresAt":"2020-01-01T00:00:00Z"}),
            json!({"visibility":"private","retention":"durable","allowedUses":["retrieval"]}),
        ] {
            let (store, _, retrieval) = seeded_with_policy(policy);
            let model = Arc::new(RecordingModel::new(true));
            let (provider, index) = provider(store, model.clone());
            let mut req = request(&retrieval);
            req.requester.actor.id = Id::from("stranger");
            assert_eq!(
                block_on(provider.backfill_embeddings(&req))
                    .unwrap()
                    .embedded,
                0
            );
            assert!(model.inputs.lock().unwrap().is_empty());
            assert!(block_on(index.embedded_ids()).unwrap().is_empty());
        }
        let (store, _, retrieval) = seeded();
        let model = Arc::new(RecordingModel::new(true));
        let (provider, index) = provider(store, model.clone());
        let mut req = request(&retrieval);
        req.scope.session = Some("other".into());
        assert_eq!(
            block_on(provider.backfill_embeddings(&req))
                .unwrap()
                .embedded,
            0
        );
        assert!(model.inputs.lock().unwrap().is_empty());
        assert!(block_on(index.embedded_ids()).unwrap().is_empty());
    }

    #[test]
    fn a_generation_transition_during_inference_prevents_vector_insertion() {
        let (store, mut manifest, retrieval) = seeded();
        let model = Arc::new(RecordingModel::new(true));
        manifest.attempt_id = "during-inference".into();
        let transitioning_store = store.clone();
        *model.before_return.lock().unwrap() = Some(Box::new(move || {
            transitioning_store
                .begin_code_source(manifest, Some("first"))
                .now_or_never()
                .expect("SQLite transition is immediately ready")
                .unwrap();
        }));
        let (provider, index) = provider(store, model.clone());
        let _ = block_on(provider.backfill_embeddings(&request(&retrieval)));
        assert_eq!(
            model.inputs.lock().unwrap().len(),
            1,
            "race must actually happen during inference"
        );
        assert!(block_on(index.embedded_ids()).unwrap().is_empty());
    }

    #[test]
    fn inference_failure_is_reported_and_retry_converges() {
        let (store, _, retrieval) = seeded();
        let model = Arc::new(RecordingModel::new(true));
        model.fail.store(true, Ordering::SeqCst);
        let (provider, index) = provider(store, model.clone());
        assert!(block_on(provider.backfill_embeddings(&request(&retrieval))).is_err());
        assert!(block_on(index.embedded_ids()).unwrap().is_empty());
        model.fail.store(false, Ordering::SeqCst);
        let retry = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
        assert_eq!(retry.embedded, 1);
        assert_eq!(retry.remaining, 0);
        assert_eq!(model.inputs.lock().unwrap().len(), 2);
    }

    #[test]
    fn local_backfill_accepts_all_semantic_code_roles() {
        for role in ["symbol", "container_context", "residual"] {
            let (store, _, retrieval) = seeded_with_role(
                json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}),
                role,
            );
            let model = Arc::new(RecordingModel::new(true));
            let (provider, index) = provider(store, model.clone());
            let report = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
            assert_eq!(report.code_embedded, 1, "{role} should be embedded");
            assert_eq!(*model.inputs.lock().unwrap(), vec!["hello"]);
            assert!(
                block_on(index.vector_for_target(&Id::from("unit")))
                    .unwrap()
                    .is_some()
            );
            assert!(
                block_on(index.vector_for_target(&Id::from("snapshot")))
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn a_legacy_code_chunk_without_validated_generation_is_not_embedded() {
        let (store, _, retrieval) = seeded();
        let mut legacy = block_on(store.get_chunk(&Id::from("unit"), &retrieval.scope))
            .unwrap()
            .unwrap();
        legacy.id = Id::from("legacy");
        legacy.code = None;
        add_unowned_source_document(&store, &retrieval.scope, SourceDocumentKind::Code);
        legacy.source_id = Id::from("unowned-source");
        legacy.document_id = Id::from("unowned-document");
        block_on(store.put_chunk(legacy)).unwrap();
        let model = Arc::new(RecordingModel::new(true));
        let (provider, index) = provider(store, model.clone());
        let report = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
        assert_eq!(report.code_embedded, 1);
        assert_eq!(model.inputs.lock().unwrap().len(), 1);
        assert_eq!(
            block_on(index.embedded_ids()).unwrap(),
            [Id::from("unit")].into_iter().collect()
        );
    }

    #[test]
    fn document_backfill_remains_available_and_bounded_hash_reuse_skips_inference() {
        for local in [true, false] {
            let (store, _, retrieval) = seeded();
            add_unowned_source_document(&store, &retrieval.scope, SourceDocumentKind::Markdown);
            let template = block_on(store.get_chunk(&Id::from("unit"), &retrieval.scope))
                .unwrap()
                .unwrap();
            for id in ["ordinary-a", "ordinary-b"] {
                let mut chunk = template.clone();
                chunk.id = Id::from(id);
                chunk.document_id = Id::from("unowned-document");
                chunk.source_id = Id::from("unowned-source");
                chunk.kind = KnowledgeChunkKind::Paragraph;
                chunk.document_kind = Some(SourceDocumentKind::Markdown);
                chunk.code = None;
                chunk.provenance.source = "ordinary".into();
                block_on(store.put_chunk(chunk)).unwrap();
            }
            let model = Arc::new(RecordingModel::new(local));
            let (provider, index) = provider(store, model.clone());
            let mut req = request(&retrieval);
            req.source_filter = Some("ordinary".into());
            req.limit = 1;
            let first = block_on(provider.backfill_embeddings(&req)).unwrap();
            assert_eq!(first.embedded, 1);
            assert_eq!(first.new_embeddings, 1);
            assert_eq!(first.code_embedded, 0);
            assert_eq!(first.remaining, 1);
            let second = block_on(provider.backfill_embeddings(&req)).unwrap();
            assert_eq!(second.embedded, 1);
            assert_eq!(second.new_embeddings, 0);
            assert_eq!(second.reused, 1);
            assert_eq!(second.remaining, 0);
            assert_eq!(model.inputs.lock().unwrap().len(), 1);
            assert_eq!(
                block_on(index.embedded_ids()).unwrap(),
                [Id::from("ordinary-a"), Id::from("ordinary-b")]
                    .into_iter()
                    .collect()
            );
        }
    }

    #[test]
    fn code_embedding_readiness_uses_actual_local_model_vector_and_source_handles() {
        let (store, _, _) = seeded();
        let local = Arc::new(RecordingModel::new(true));
        let external = Arc::new(RecordingModel::new(false));
        let (ready, _) = provider(store.clone(), local.clone());
        let (external, _) = provider(store.clone(), external);
        let capabilities = CapabilityReport::new(CapabilityState::Supported);
        let missing_vector = EngramProviderBuilder::new(capabilities.clone())
            .knowledge(store.clone())
            .knowledge_query(store.clone())
            .embedding_provider(local.clone())
            .build();
        let missing_query = EngramProviderBuilder::new(capabilities.clone())
            .knowledge(store.clone())
            .embedding_provider(local.clone())
            .vectors(Arc::new(
                SqliteVectorIndex::open_in_memory(3)
                    .unwrap()
                    .with_embedding_space(local.embedding_space()),
            ))
            .build();
        let missing_source = EngramProviderBuilder::new(capabilities)
            .knowledge_query(store)
            .embedding_provider(local.clone())
            .vectors(Arc::new(
                SqliteVectorIndex::open_in_memory(3)
                    .unwrap()
                    .with_embedding_space(local.embedding_space()),
            ))
            .build();
        let field = |p: &EngramProvider| {
            serde_json::to_value(p.capabilities())
                .unwrap()
                .get("code_embeddings")
                .cloned()
                .expect("readiness must be reported")
        };
        let supported = serde_json::to_value(CapabilityState::Supported).unwrap();
        assert_eq!(field(&ready), supported);
        for unavailable in [external, missing_vector, missing_query, missing_source] {
            assert_ne!(
                field(&unavailable),
                supported,
                "config claims alone must not advertise readiness"
            );
        }
    }

    fn add_unowned_source_document(
        store: &SqlKnowledgeStore,
        scope: &Scope,
        kind: SourceDocumentKind,
    ) {
        let mut source = block_on(store.list_sources(scope)).unwrap().remove(0);
        source.id = Id::from("unowned-source");
        source.metadata = None;
        block_on(store.put_source(source)).unwrap();
        let mut document = block_on(store.list_documents(scope)).unwrap().remove(0);
        document.id = Id::from("unowned-document");
        document.source_id = Id::from("unowned-source");
        document.kind = kind;
        block_on(store.put_document(document)).unwrap();
    }

    #[test]
    fn expiry_during_inference_prevents_vector_insertion() {
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(2);
        let (store, _, retrieval) = seeded_with_policy(json!({
            "visibility":"workspace", "retention":"durable", "allowedUses":["retrieval"],
            "expiresAt": expires_at.to_rfc3339(),
        }));
        let model = Arc::new(RecordingModel::new(true));
        *model.before_return.lock().unwrap() = Some(Box::new(move || {
            let remaining = (expires_at - chrono::Utc::now())
                .to_std()
                .unwrap_or_default();
            std::thread::sleep(remaining + std::time::Duration::from_millis(10));
        }));
        let (provider, index) = provider(store, model.clone());
        let result = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
        assert_eq!(
            model.inputs.lock().unwrap().len(),
            1,
            "must expire after inference starts"
        );
        assert_eq!(result.embedded, 0);
        assert!(block_on(index.embedded_ids()).unwrap().is_empty());
    }

    struct FailOnceVector {
        inner: Arc<SqliteVectorIndex>,
        fail: AtomicBool,
    }
    #[async_trait::async_trait]
    impl VectorIndex for FailOnceVector {
        fn embedding_space(&self) -> &EmbeddingSpace {
            self.inner.embedding_space()
        }
        async fn insert(
            &self,
            id: &Id,
            space: &EmbeddingSpace,
            vector: Vec<f32>,
        ) -> CoreResult<()> {
            if self.fail.swap(false, Ordering::SeqCst) {
                return Err(CoreError::Adapter {
                    adapter: "test_vector".into(),
                    message: "insert failed".into(),
                });
            }
            VectorIndex::insert(self.inner.as_ref(), id, space, vector).await
        }
        async fn search(
            &self,
            space: &EmbeddingSpace,
            vector: Vec<f32>,
            limit: usize,
        ) -> CoreResult<Vec<(Id, f32)>> {
            VectorIndex::search(self.inner.as_ref(), space, vector, limit).await
        }
        async fn vector_for_target(&self, id: &Id) -> CoreResult<Option<Vec<f32>>> {
            self.inner.vector_for_target(id).await
        }
        async fn embedded_ids(&self) -> CoreResult<std::collections::HashSet<Id>> {
            self.inner.embedded_ids().await
        }
        async fn delete_target(&self, id: &Id) -> CoreResult<()> {
            self.inner.delete_target(id).await
        }
        async fn gc_orphan_targets(&self, ids: &[Id]) -> CoreResult<usize> {
            self.inner.gc_orphan_targets(ids).await
        }
        async fn clear(&self) -> CoreResult<()> {
            VectorIndex::clear(self.inner.as_ref()).await
        }
    }

    #[test]
    fn vector_insertion_failure_is_reported_and_retry_converges() {
        let (store, _, retrieval) = seeded();
        let model = Arc::new(RecordingModel::new(true));
        let index = Arc::new(FailOnceVector {
            inner: Arc::new(
                SqliteVectorIndex::open_in_memory(3)
                    .unwrap()
                    .with_embedding_space(model.embedding_space()),
            ),
            fail: AtomicBool::new(true),
        });
        let provider =
            EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
                .knowledge(store.clone())
                .knowledge_query(store)
                .embedding_provider(model)
                .vectors(index.clone())
                .build();
        let error = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap_err();
        assert!(matches!(error, CoreError::Adapter { adapter, .. } if adapter == "test_vector"));
        assert!(block_on(index.embedded_ids()).unwrap().is_empty());
        let report = block_on(provider.backfill_embeddings(&request(&retrieval))).unwrap();
        assert_eq!(report.embedded, 1);
        assert_eq!(report.code_embedded, 1);
        assert_eq!(report.remaining, 0);
        assert_eq!(
            block_on(index.embedded_ids()).unwrap(),
            [Id::from("unit")].into_iter().collect()
        );
    }

    #[test]
    fn large_passages_have_bounded_model_inputs_and_batches_without_mutating_source() {
        let (store, _, retrieval) = seeded();
        add_unowned_source_document(&store, &retrieval.scope, SourceDocumentKind::Markdown);
        let template = block_on(store.get_chunk(&Id::from("unit"), &retrieval.scope))
            .unwrap()
            .unwrap();
        let mut originals = Vec::new();
        for i in 0..65 {
            let mut chunk = template.clone();
            chunk.id = Id::from(format!("large-{i:03}"));
            chunk.document_id = Id::from("unowned-document");
            chunk.source_id = Id::from("unowned-source");
            chunk.kind = KnowledgeChunkKind::Paragraph;
            chunk.document_kind = Some(SourceDocumentKind::Markdown);
            chunk.code = None;
            chunk.provenance.source = "large-fixture".into();
            chunk.text = format!("passage {i:03}: {}", "λ".repeat(32_768));
            // Independent full-content identities ensure 65 inference groups,
            // even though much of the passage body is shared.
            chunk.content_hash = format!("large-passage-{i:03}");
            originals.push((chunk.id.clone(), chunk.text.clone()));
            block_on(store.put_chunk(chunk)).unwrap();
        }
        let model = Arc::new(RecordingModel::new(true));
        let (provider, index) = provider(store.clone(), model.clone());
        let mut req = request(&retrieval);
        req.limit = 65;
        req.source_filter = Some("large-fixture".into());
        let report = block_on(provider.backfill_embeddings(&req)).unwrap();
        assert_eq!(report.embedded, 65);
        assert_eq!(report.new_embeddings, 65);
        assert_eq!(report.code_embedded, 0);
        assert_eq!(report.remaining, 0);
        let inputs = model.inputs.lock().unwrap();
        assert_eq!(inputs.len(), 65);
        assert!(inputs.iter().all(|text| text.chars().count() == 8192));
        drop(inputs);
        let batches = model.batch_sizes.lock().unwrap();
        assert_eq!(batches.iter().sum::<usize>(), 65);
        assert!(batches.len() >= 2);
        assert!(batches.iter().all(|size| (1..=64).contains(size)));
        drop(batches);
        assert_eq!(
            block_on(index.embedded_ids()).unwrap(),
            originals.iter().map(|(id, _)| id.clone()).collect()
        );
        for (id, text) in &originals {
            assert_eq!(
                &block_on(store.get_chunk(id, &retrieval.scope))
                    .unwrap()
                    .unwrap()
                    .text,
                text
            );
        }
        assert_eq!(
            block_on(store.get_chunk(&Id::from("unit"), &retrieval.scope))
                .unwrap()
                .unwrap()
                .text,
            "hello"
        );
        let repeat = block_on(provider.backfill_embeddings(&req)).unwrap();
        assert_eq!(repeat.embedded, 0);
        assert_eq!(repeat.remaining, 0);
        assert_eq!(model.inputs.lock().unwrap().len(), 65);
        assert_eq!(model.batch_sizes.lock().unwrap().iter().sum::<usize>(), 65);
    }
}
