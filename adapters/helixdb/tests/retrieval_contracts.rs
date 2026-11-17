//! Real provider candidates, vector persistence and negative identity/model boundaries.
#[path = "support/data_fixture.rs"]
mod fixture;
use engram_domain::*;
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository, TaxonomyRepository};
use engram_memory::MemoryRepository;
use engram_retrieval::{RetrievalIndex, VectorIndex};
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use engram_store_helixdb::retrieval::QueryVectorProvider;
use fixture::*;
use futures::executor::block_on;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct PolicyGate;
impl PolicyAuthorizer for PolicyGate {
    fn can_write(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_forget(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(&self, _: &Requester, _: &Scope, p: &Policy) -> CoreResult<()> {
        if p.allowed_uses.contains(&AllowedUse::Retrieval) {
            Ok(())
        } else {
            Err(CoreError::InvalidRequest {
                reason: "fixture host denies retrieval".into(),
            })
        }
    }
}
struct QueryEmbedding {
    calls: Arc<AtomicUsize>,
    space: EmbeddingSpace,
}
impl QueryVectorProvider for QueryEmbedding {
    fn query_vector(&self, _: &RetrievalRequest) -> CoreResult<(EmbeddingSpace, Vec<f32>)> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((self.space.clone(), vec![1.0, 0.0]))
    }
}
fn request(scope: Scope) -> RetrievalRequest {
    serde_json::from_value(json!({"query":"needle","scope":scope,"requester":{"actor":{"id":"actor","kind":"agent"}},"limit":1,"modes":["keyword"]})).unwrap()
}
fn space() -> EmbeddingSpace {
    EmbeddingSpace::new("explicit-test-input", "model", 2, "query", Some("l2"))
}
fn exercise(mode: Mode) {
    block_on(async {
        let mut disabled = Fixture::new(mode);
        let calls = Arc::new(AtomicUsize::new(0));
        let index = disabled
            .store
            .retrieval_index(Arc::new(PolicyGate))
            .with_query_vector_provider(Arc::new(QueryEmbedding {
                calls: calls.clone(),
                space: space(),
            }));
        let mut semantic = request(disabled.partition.scope.clone());
        semantic.modes = vec![RetrievalMode::Semantic];
        assert!(index.retrieve_candidates(&semantic).await.is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "disabled vectors must not invoke model/query-vector collaborator"
        );
        assert!(disabled.store.vector_index().is_err());
        disabled.reopen();
        disabled.store.close().unwrap();
        drop(index);
        drop(disabled);
        let mut f = Fixture::with_space(mode, Some(space()));
        let scope = f.partition.scope.clone();
        let mut denied = memory("denied");
        denied.content.text = "needle needle needle forbidden".into();
        denied.policy.allowed_uses.clear();
        f.store.put_memory(denied).await.unwrap();
        let mut allowed = memory("allowed");
        allowed.content.text = "needle permitted".into();
        f.store.put_memory(allowed.clone()).await.unwrap();
        let mut expired = memory("expired");
        expired.content.text = "needle stale".into();
        expired.policy.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        f.store.put_memory(expired).await.unwrap();
        let original_scope = scope.clone();
        let original_store = f.partition.logical_store.clone();
        for boundary in ["scope", "logical-store"] {
            let mut other_scope = original_scope.clone();
            let mut other_store = original_store.clone();
            if boundary == "scope" {
                other_scope.subject = Some(String::new());
                other_scope.session = Some("foreign-session".into());
                other_scope.environment = Some("foreign-environment".into());
            } else {
                other_store.push_str("-foreign");
            }
            f.switch_partition(other_scope, other_store);
            let mut decoy = memory(&format!("foreign-decoy-{boundary}"));
            decoy.scope = f.partition.scope.clone();
            decoy.content.text = "needle needle needle needle foreign top-ranked decoy".into();
            f.store.put_memory(decoy.clone()).await.unwrap();
            let foreign_vectors = f.store.vector_index().unwrap();
            foreign_vectors
                .insert(&decoy.id, &space(), vec![1.0, 0.0])
                .await
                .unwrap();
            drop(foreign_vectors);
            f.switch_partition(original_scope.clone(), original_store.clone());
        }
        let index = f.store.retrieval_index(Arc::new(PolicyGate));
        let q = request(scope.clone());
        let hits = index.retrieve_candidates(&q).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].target_id, "allowed",
            "policy eligibility must precede native top-k, even with forbidden high-scoring rows"
        );
        let mut foreign = q.clone();
        foreign.scope.subject = Some("foreign".into());
        assert!(
            index
                .retrieve_candidates(&foreign)
                .await
                .unwrap()
                .is_empty()
        );
        let vectors = f.store.vector_index().unwrap();
        vectors
            .insert(&Id::from("allowed"), &space(), vec![1.0, 0.0])
            .await
            .unwrap();
        vectors
            .insert(&Id::from("denied"), &space(), vec![1.0, 0.0])
            .await
            .unwrap();
        vectors
            .insert(&Id::from("orphan"), &space(), vec![0.0, 1.0])
            .await
            .unwrap();
        let mut wrong = space();
        wrong.model = "foreign".into();
        assert!(
            vectors
                .insert(&Id::from("bad"), &wrong, vec![1.0, 0.0])
                .await
                .is_err()
        );
        assert!(vectors.search(&wrong, vec![1.0, 0.0], 2).await.is_err());
        assert!(
            vectors
                .insert(&Id::from("bad"), &space(), vec![1.0])
                .await
                .is_err()
        );
        assert!(
            vectors
                .search(&space(), vec![f32::NAN, 0.0], 2)
                .await
                .is_err()
        );
        assert_eq!(
            vectors
                .vector_for_target(&Id::from("allowed"))
                .await
                .unwrap(),
            Some(vec![1.0, 0.0])
        );
        assert_eq!(vectors.embedded_ids().await.unwrap().len(), 3);
        assert_eq!(
            vectors
                .search(&space(), vec![1.0, 0.0], 3)
                .await
                .unwrap()
                .len(),
            3
        );
        let query_calls = Arc::new(AtomicUsize::new(0));
        let semantic_index = f
            .store
            .retrieval_index(Arc::new(PolicyGate))
            .with_query_vector_provider(Arc::new(QueryEmbedding {
                calls: query_calls.clone(),
                space: space(),
            }));
        let mut semantic = q.clone();
        semantic.modes = vec![RetrievalMode::Semantic];
        let candidates = semantic_index.retrieve_candidates(&semantic).await.unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|v| v.target_id.as_str())
                .collect::<Vec<_>>(),
            vec!["allowed"]
        );
        assert_eq!(query_calls.load(Ordering::SeqCst), 1);
        drop(vectors);
        drop(index);
        drop(semantic_index);
        f.reopen();
        let vectors = f.store.vector_index().unwrap();
        assert_eq!(
            vectors
                .vector_for_target(&Id::from("allowed"))
                .await
                .unwrap(),
            Some(vec![1.0, 0.0])
        );
        assert_eq!(
            vectors
                .gc_orphan_targets(&[Id::from("allowed")])
                .await
                .unwrap(),
            2
        );
        assert_eq!(vectors.embedded_ids().await.unwrap().len(), 1);
        vectors.delete_target(&Id::from("allowed")).await.unwrap();
        assert!(
            vectors
                .search(&space(), vec![1.0, 0.0], 2)
                .await
                .unwrap()
                .is_empty()
        );
        vectors.clear().await.unwrap();
        f.store
            .put_graph(make("graph", json!({"name":"graph"})))
            .await
            .unwrap();
        f.store.put_entity(entity("same-id", &[])).await.unwrap();
        let marker = f.partition.clone();
        assert!(
            f.store
                .get_memory(&allowed.id, &marker.scope)
                .await
                .unwrap()
                .is_some()
        );
        println!(
            "TASK005 retrieval mode={mode:?}: native scoped text/vector, policy-before-topk, complete embedding identity, durable vector CRUD/GC and disabled model boundary verified"
        );
    });
}
#[cfg(feature = "embedded")]
#[test]
fn retrieval_embedded() {
    exercise(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn retrieval_service() {
    exercise(Mode::Service)
}

fn target_ids(rows: Vec<RetrievalResult>) -> std::collections::BTreeSet<String> {
    rows.into_iter().map(|v| v.target_id).collect()
}
fn family_fidelity(mode: Mode) {
    block_on(async {
        let mut f = Fixture::with_space(mode, Some(space()));
        let scope = f.partition.scope.clone();
        let at = |value: &str| {
            chrono::DateTime::parse_from_rfc3339(value)
                .unwrap()
                .with_timezone(&chrono::Utc)
        };
        let start = at("2026-11-17T01:00:00Z");
        let end = at("2026-11-17T02:00:00Z");
        let mut memory = memory("family-memory");
        memory.content.text = "familyneedle memory".into();
        memory.provenance.confidence = Some(0.9);
        f.store.put_memory(memory).await.unwrap();
        f.store
            .put_source(make("family-source", json!({"kind":"generated"})))
            .await
            .unwrap();
        f.store
            .put_document(make(
                "family-doc",
                json!({"sourceId":"family-source","kind":"text","contentHash":"doc-hash"}),
            ))
            .await
            .unwrap();
        let chunk: KnowledgeChunk = make(
            "family-chunk",
            json!({"documentId":"family-doc","sourceId":"family-source","kind":"paragraph","text":"familyneedle chunk","contentHash":"chunk-hash","concepts":[{"id":"topic"}]}),
        );
        f.store.put_chunk(chunk.clone()).await.unwrap();
        let mut denied_chunk = chunk.clone();
        denied_chunk.id = Id::from("denied-chunk");
        denied_chunk.policy.allowed_uses.clear();
        f.store.put_chunk(denied_chunk).await.unwrap();
        let graph: KnowledgeGraph = make("family-graph", json!({"name":"governed graph"}));
        f.store.put_graph(graph.clone()).await.unwrap();
        let mut denied_graph = graph.clone();
        denied_graph.id = Id::from("denied-graph");
        denied_graph.policy.allowed_uses.clear();
        f.store.put_graph(denied_graph).await.unwrap();
        let entity: KnowledgeEntity = make(
            "family-entity",
            json!({"name":"familyneedle entity","kind":"concept","graphId":"family-graph","conceptRefs":[{"id":"topic"}],"validFrom":start,"validUntil":end}),
        );
        f.store.put_entity(entity.clone()).await.unwrap();
        let mut denied_entity = entity.clone();
        denied_entity.id = Id::from("denied-entity");
        denied_entity.graph_id = Some(Id::from("denied-graph"));
        f.store.put_entity(denied_entity).await.unwrap();
        let mut missing = entity.clone();
        missing.id = Id::from("missing-parent");
        missing.graph_id = Some(Id::from("absent-graph"));
        f.store.put_entity(missing).await.unwrap();
        let mut graphless = entity.clone();
        graphless.id = Id::from("graphless");
        graphless.graph_id = None;
        graphless.concept_refs.clear();
        graphless.valid_from = None;
        graphless.valid_until = None;
        f.store.put_entity(graphless).await.unwrap();
        let relationship: KnowledgeRelationship = make(
            "family-relationship",
            json!({"graphId":"family-graph","subject":{"id":"family-entity"},"object":{"id":"graphless"},"predicate":"familyneedle"}),
        );
        f.store
            .put_relationship(relationship.clone())
            .await
            .unwrap();
        let mut denied_relationship = relationship;
        denied_relationship.id = Id::from("denied-relationship");
        denied_relationship.graph_id = Some(Id::from("denied-graph"));
        f.store.put_relationship(denied_relationship).await.unwrap();
        let scheme: ConceptScheme =
            make("family-scheme", json!({"uri":"urn:family","version":"1"}));
        f.store.put_concept_scheme(scheme.clone()).await.unwrap();
        let mut denied_scheme = scheme;
        denied_scheme.id = Id::from("denied-scheme");
        denied_scheme.policy.allowed_uses.clear();
        f.store.put_concept_scheme(denied_scheme).await.unwrap();
        let concept: Concept = make(
            "topic",
            json!({"uri":"urn:topic","schemeId":"family-scheme","prefLabel":{"value":"familyneedle topic"},"altLabels":[{"value":"alternateneedle"}],"definition":"definitionneedle","status":"active"}),
        );
        f.store.put_concept(concept.clone()).await.unwrap();
        let mut denied_concept = concept;
        denied_concept.id = Id::from("denied-concept");
        denied_concept.scheme_id = Id::from("denied-scheme");
        f.store.put_concept(denied_concept).await.unwrap();
        let vectors = f.store.vector_index().unwrap();
        for id in [
            "family-memory",
            "family-chunk",
            "family-entity",
            "family-relationship",
            "topic",
            "graphless",
            "denied-entity",
            "denied-chunk",
            "denied-relationship",
            "denied-concept",
            "missing-parent",
        ] {
            vectors
                .insert(&Id::from(id), &space(), vec![1.0, 0.0])
                .await
                .unwrap();
        }
        drop(vectors);
        let expected = [
            "family-memory",
            "family-chunk",
            "family-entity",
            "family-relationship",
            "topic",
            "graphless",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
        for reopen in [false, true] {
            if reopen {
                f.reopen();
            }
            let calls = Arc::new(AtomicUsize::new(0));
            let index = f
                .store
                .retrieval_index(Arc::new(PolicyGate))
                .with_query_vector_provider(Arc::new(QueryEmbedding {
                    calls: calls.clone(),
                    space: space(),
                }));
            let mut q = request(scope.clone());
            q.query = "familyneedle".into();
            q.limit = Some(50);
            assert_eq!(
                target_ids(index.retrieve_candidates(&q).await.unwrap()),
                expected,
                "all canonical family shapes survive mixed retrieval; denied/missing ancestry does not erase memory"
            );
            let mut semantic = q.clone();
            semantic.modes = vec![RetrievalMode::Semantic];
            assert_eq!(
                target_ids(index.retrieve_candidates(&semantic).await.unwrap()),
                expected,
                "semantic uses the identical typed policy projection"
            );
            let mut confidence = q.clone();
            confidence.filters =
                Some(serde_json::from_value(json!({"minConfidence":0.85})).unwrap());
            assert_eq!(
                target_ids(index.retrieve_candidates(&confidence).await.unwrap()),
                ["family-memory".into()].into_iter().collect()
            );
            confidence.filters.as_mut().unwrap().min_confidence = Some(0.95);
            assert!(
                index
                    .retrieve_candidates(&confidence)
                    .await
                    .unwrap()
                    .is_empty()
            );
            let mut concept_filter = q.clone();
            concept_filter.filters =
                Some(serde_json::from_value(json!({"conceptIds":["topic"]})).unwrap());
            assert_eq!(
                target_ids(index.retrieve_candidates(&concept_filter).await.unwrap()),
                [
                    "family-chunk".into(),
                    "family-entity".into(),
                    "topic".into()
                ]
                .into_iter()
                .collect()
            );
            concept_filter.filters.as_mut().unwrap().concept_ids = vec![Id::from("not-present")];
            assert!(
                index
                    .retrieve_candidates(&concept_filter)
                    .await
                    .unwrap()
                    .is_empty()
            );
            for (instant, has_entity) in [
                (start - chrono::Duration::seconds(1), false),
                (start, true),
                (end, false),
            ] {
                let mut temporal = q.clone();
                temporal.filters = Some(serde_json::from_value(json!({"asOf":instant})).unwrap());
                let ids = target_ids(index.retrieve_candidates(&temporal).await.unwrap());
                assert_eq!(
                    ids.contains("family-entity"),
                    has_entity,
                    "entity valid-time is start inclusive/end exclusive"
                );
                assert!(ids.contains("family-memory"));
            }
            for text in ["definitionneedle", "alternateneedle"] {
                let mut query = q.clone();
                query.query = text.into();
                assert_eq!(
                    target_ids(index.retrieve_candidates(&query).await.unwrap()),
                    ["topic".into()].into_iter().collect(),
                    "concept label/definition text honors scheme policy"
                );
            }
        }
        println!(
            "TASK005 corrected family mode={mode:?}: canonical graph/scheme policy, mixed native keyword+semantic/reopen, confidence/concept refs/intervals passed"
        );
    });
}
#[cfg(feature = "embedded")]
#[test]
fn canonical_family_embedded() {
    family_fidelity(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn canonical_family_service() {
    family_fidelity(Mode::Service)
}
