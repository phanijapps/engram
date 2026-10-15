#![cfg(feature = "sqlite")]
//! Real canonical/segment authority with counted deterministic lane/model ports.
use async_trait::async_trait;
use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_integration::{
    CapabilityReport, EngramProvider, EngramProviderBuilder,
    code_candidates::collect, code_representation::code_representation,
};
use engram_knowledge::{CodeSegmentRepository, CodeSourceRepository};
use engram_retrieval::CodeRetrievalLane;
use engram_runtime::CoreResult;
use futures::executor::block_on;
use serde_json::json;
use std::sync::{Arc, Mutex, atomic::Ordering};

#[path = "support/code_authority.rs"]
mod authority;
use authority::*;

#[test]
fn candidate_depth_is_independent_of_final_output_and_disabled_lanes_are_uncalled() {
    let f = fixture();
    assert!(
        f.lexical
            .rows
            .iter()
            .any(|r| r.content.contains("PRIVATE_MARKER")),
        "the untrusted lane must actually offer denied content"
    );
    let result = block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &f.config,
    ))
    .unwrap();
    assert!(
        result
            .candidates
            .iter()
            .map(|c| &c.parent.id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 10
    );
    assert_eq!(f.lexical.calls.lock().unwrap()[0].limit, Some(50));
    assert_eq!(f.model.queries.load(Ordering::SeqCst), 1);
    assert!(f.graph.calls.lock().unwrap().is_empty());
    assert!(
        result
            .candidates
            .iter()
            .filter(|c| c.parent.code.as_ref().unwrap().role == CodeChunkRole::Symbol)
            .all(|c| c.segment.source_category == CodeSourceCategory::Implementation)
    );
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.segment.file.path != "secret.ts")
    );
    assert!(result.phase_ms.contains_key("vector_search"));
    let mut config = f.config.clone();
    config.retrieval.enabled_lanes = vec![CodeRetrievalLane::Lexical];
    config.retrieval.graph_hops = 0;
    config.fusion.source_weights.insert("lexical".into(), 0.0);
    // A different positive enabled lane is required by config validation. Keep
    // lexical zero and vector positive: zero lexical weight still calls it.
    config
        .retrieval
        .enabled_lanes
        .push(CodeRetrievalLane::Vector);
    block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &config,
    ))
    .unwrap();
    assert_eq!(f.lexical.calls.lock().unwrap().len(), 2);
    config.retrieval.enabled_lanes = vec![CodeRetrievalLane::Lexical];
    config.fusion.source_weights.clear();
    block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &config,
    ))
    .unwrap();
    assert_eq!(
        f.model.queries.load(Ordering::SeqCst),
        2,
        "disabled vector lane must perform no inference"
    );
}

#[test]
fn source_mutation_revokes_warm_candidates_and_strict_scope_repository_match() {
    let f = fixture();
    let first = block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &f.config,
    ))
    .unwrap();
    assert!(!first.candidates.is_empty());
    let mut wrong = query();
    wrong.repository = Some("different-repository".into());
    assert!(
        block_on(collect(
            &f.provider,
            &wrong,
            &f.scope,
            &f.requester,
            &f.config
        ))
        .unwrap()
        .candidates
        .is_empty()
    );
    let mut scope = f.scope.clone();
    scope.workspace = Some("sibling".into());
    assert!(
        block_on(collect(
            &f.provider,
            &query(),
            &scope,
            &f.requester,
            &f.config
        ))
        .unwrap()
        .candidates
        .is_empty()
    );
    let file = first.candidates[0].segment.file.clone();
    let mut source = block_on(f.store.get_code_source(&file)).unwrap().unwrap();
    let prior = source.attempt_id.clone();
    source.attempt_id = "refresh-after-warm-query".into();
    source.state = CodeSourceState::Updating;
    block_on(f.store.begin_code_source(source, Some(&prior))).unwrap();
    assert!(
        block_on(collect(
            &f.provider,
            &query(),
            &f.scope,
            &f.requester,
            &f.config
        ))
        .unwrap()
        .candidates
        .is_empty()
    );
    assert!(block_on(f.store.code_index_revision()).unwrap() > first.source_revision);
}

#[test]
fn malformed_embedded_queries_fail_before_tokenization_inference_or_lane_access() {
    let f = fixture();
    let mut query = query();
    query.repository = Some("x".repeat(2049));
    assert!(
        block_on(collect(
            &f.provider,
            &query,
            &f.scope,
            &f.requester,
            &f.config
        ))
        .is_err()
    );
    assert_eq!(f.model.queries.load(Ordering::SeqCst), 0);
    assert!(f.lexical.calls.lock().unwrap().is_empty());
}

#[test]
fn expansion_is_anchored_globally_bounded_and_zero_hops_skips_all_graph_calls() {
    let f = fixture();
    let mut config = f.config.clone();
    config.retrieval.graph_neighbor_limit = 2;
    config.retrieval.graph_hops = 2;
    let result = block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &config,
    ))
    .unwrap();
    let calls = f.expansion.calls.lock().unwrap().clone();
    assert!(!calls.is_empty());
    assert!(calls.len() <= 2);
    assert!(calls.iter().all(|&limit| limit <= 2));
    assert!(result.candidate_counts["graph_expansion"] <= 50);
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.segment.file.path != "secret.ts")
    );
    f.expansion.calls.lock().unwrap().clear();
    config.retrieval.graph_hops = 0;
    block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &config,
    ))
    .unwrap();
    assert!(f.expansion.calls.lock().unwrap().is_empty());
    let mut absent = query();
    absent.repository = Some("missing".into());
    config.retrieval.graph_hops = 1;
    block_on(collect(
        &f.provider,
        &absent,
        &f.scope,
        &f.requester,
        &config,
    ))
    .unwrap();
    assert!(
        f.expansion.calls.lock().unwrap().is_empty(),
        "no retrieved authorized anchors means no expansion"
    );
}

fn without_graph(f: &Fixture, lexical: Arc<Lane>) -> EngramProvider {
    EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
        .knowledge(f.store.clone())
        .knowledge_query(f.store.clone())
        .embedding_provider(f.model.clone())
        .vectors(f.provider.vectors().unwrap().clone())
        .code_vectors(f.provider.code_vectors().unwrap().clone())
        .code_retrieval_lane(CodeRetrievalLane::Lexical, lexical)
        .build()
}

#[test]
fn missing_requested_expansion_reports_failure_only_when_anchors_exist() {
    let f = fixture();
    let provider = without_graph(&f, f.lexical.clone());
    let result = block_on(collect(
        &provider,
        &query(),
        &f.scope,
        &f.requester,
        &f.config,
    ))
    .unwrap();
    assert!(!result.candidates.is_empty());
    assert!(
        result
            .source_failures
            .iter()
            .any(|failure| failure.source == "graph_expansion")
    );
    let mut config = f.config.clone();
    config.retrieval.graph_hops = 0;
    assert!(
        block_on(collect(
            &provider,
            &query(),
            &f.scope,
            &f.requester,
            &config
        ))
        .unwrap()
        .source_failures
        .is_empty()
    );
    let mut absent = query();
    absent.repository = Some("missing".into());
    assert!(
        block_on(collect(
            &provider,
            &absent,
            &f.scope,
            &f.requester,
            &f.config
        ))
        .unwrap()
        .source_failures
        .is_empty()
    );
}

#[test]
fn denied_leading_hit_and_duplicates_do_not_inflate_scores_or_misstate_fusion_ranks() {
    let f = fixture();
    let denied = f
        .lexical
        .rows
        .iter()
        .find(|row| row.content.contains("PRIVATE_MARKER"))
        .unwrap()
        .clone();
    let public = f
        .lexical
        .rows
        .iter()
        .filter(|row| !row.content.contains("PRIVATE_MARKER"))
        .cloned()
        .collect::<Vec<_>>();
    let mut rows = vec![denied];
    rows.push(public[0].clone());
    rows.extend(public);
    let lane = Arc::new(Lane {
        rows,
        calls: Mutex::new(vec![]),
    });
    let provider = without_graph(&f, lane);
    let mut config = f.config.clone();
    config.retrieval.enabled_lanes = vec![CodeRetrievalLane::Lexical];
    config.retrieval.graph_hops = 0;
    let result = block_on(collect(
        &provider,
        &query(),
        &f.scope,
        &f.requester,
        &config,
    ))
    .unwrap();
    let first = &result.candidates[0];
    assert_eq!(first.fusion_traces.len(), 1);
    assert_eq!(first.fusion_traces[0].source_rank, Some(1));
    assert!((first.fusion_score - 1.0 / 61.0).abs() < 1e-7);
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.segment.file.path != "secret.ts")
    );
}

#[test]
fn retired_source_does_not_require_an_embedding_reindex_that_cannot_repair_it() {
    let f = fixture();
    std::fs::remove_file(f._root.path().join("public.ts")).unwrap();
    std::fs::write(
        f._root.path().join("live.ts"),
        "export function stillLive() { return 'LIVE'; }\n",
    )
    .unwrap();
    let options = ScanOptions {
        scope: f.scope.clone(),
        policy: serde_json::from_value(
            json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}),
        )
        .unwrap(),
        actor: f.requester.actor.clone(),
        source_name: "fixture".into(),
        max_bytes: 0,
        manifest: Default::default(),
        scan_filter: Default::default(),
    };
    assert_eq!(
        scan_repository(f._root.path(), &options, f.store.as_ref(), |_| {})
            .unwrap()
            .0
            .errors,
        0
    );
    assert_eq!(
        block_on(f.provider.backfill_embeddings(&EmbeddingBackfillRequest {
            scope: f.scope.clone(),
            requester: f.requester.clone(),
            limit: 1024,
            source_filter: None,
        }))
        .unwrap()
        .remaining,
        0
    );
    let result = block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &f.config,
    ))
    .unwrap();
    assert!(
        !result.reindex_required,
        "retired source has no active representation to repair"
    );
    assert!(!result.candidates.is_empty());
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.segment.file.path == "live.ts")
    );
}

struct OrderedVector {
    inner: Arc<dyn engram_retrieval::VectorIndex>,
    ids: Vec<Id>,
}
#[async_trait]
impl engram_retrieval::VectorIndex for OrderedVector {
    fn embedding_space(&self) -> &EmbeddingSpace {
        self.inner.embedding_space()
    }
    async fn insert(&self, id: &Id, space: &EmbeddingSpace, vector: Vec<f32>) -> CoreResult<()> {
        self.inner.insert(id, space, vector).await
    }
    async fn search(
        &self,
        _: &EmbeddingSpace,
        _: Vec<f32>,
        limit: usize,
    ) -> CoreResult<Vec<(Id, f32)>> {
        Ok(self
            .ids
            .iter()
            .take(limit)
            .enumerate()
            .map(|(i, id)| (id.clone(), i as f32))
            .collect())
    }
    async fn delete_target(&self, id: &Id) -> CoreResult<()> {
        self.inner.delete_target(id).await
    }
    async fn gc_orphan_targets(&self, ids: &[Id]) -> CoreResult<usize> {
        self.inner.gc_orphan_targets(ids).await
    }
    async fn clear(&self) -> CoreResult<()> {
        self.inner.clear().await
    }
}

#[test]
fn actual_denied_code_vector_ahead_of_public_hits_has_truthful_admitted_rrf_ranks() {
    let f = fixture();
    let mut private = f.requester.clone();
    private.actor.id = "private-owner".into();
    // The owner indexes its own private source. Another actor's query must
    // remove that real vector before RRF and must not expose its source facts.
    block_on(f.provider.backfill_embeddings(&EmbeddingBackfillRequest {
        scope: f.scope.clone(),
        requester: private.clone(),
        limit: 1024,
        source_filter: None,
    }))
    .unwrap();
    let source = block_on(f.store.list_code_sources(&f.scope))
        .unwrap()
        .into_iter()
        .find(|s| s.file.path == "secret.ts")
        .unwrap();
    let representation = code_representation(f.model.as_ref(), &f.config.index).unwrap();
    let set = block_on(f.store.load_current_code_segments(
        &source.file,
        &representation.id,
        &private,
    ))
    .unwrap()
    .unwrap();
    let denied = Id::from(set.segments[0].id.clone());
    let inner = f.provider.code_vectors().unwrap().clone();
    let mut other: Vec<_> = block_on(inner.embedded_ids())
        .unwrap()
        .into_iter()
        .filter(|id| id != &denied)
        .collect();
    other.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut ids = vec![denied];
    ids.extend(other);
    let index = Arc::new(OrderedVector { inner, ids });
    let provider = EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
        .knowledge(f.store.clone())
        .knowledge_query(f.store.clone())
        .embedding_provider(f.model.clone())
        .vectors(f.provider.vectors().unwrap().clone())
        .code_vectors(index)
        .build();
    let mut config = f.config.clone();
    config.retrieval.enabled_lanes = vec![CodeRetrievalLane::Vector];
    config.retrieval.graph_hops = 0;
    let result = block_on(collect(
        &provider,
        &query(),
        &f.scope,
        &f.requester,
        &config,
    ))
    .unwrap();
    assert!(!result.candidates.is_empty());
    for (rank, candidate) in result.candidates.iter().enumerate() {
        assert_eq!(candidate.fusion_traces.len(), 1);
        assert_eq!(
            candidate.fusion_traces[0].source_rank,
            Some((rank + 1) as u32)
        );
        assert!((candidate.fusion_score - 1.0 / (61.0 + rank as f32)).abs() < 1e-7);
        assert_ne!(candidate.segment.file.path, "secret.ts");
    }
}

#[test]
fn active_source_with_unavailable_representation_requires_reindex_and_donates_no_candidates() {
    let f = fixture();
    let first = block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &f.config,
    ))
    .unwrap();
    assert!(!first.reindex_required);
    let file = first.candidates[0].segment.file.clone();
    let lease = block_on(
        f.store
            .code_segment_inventory(&file, &first.representation_id),
    )
    .unwrap()
    .unwrap();
    let mut updating = lease.inventory.clone();
    updating.state = CodeSourceState::Updating;
    block_on(f.store.begin_code_segments(updating, Some(&lease))).unwrap();
    let changed = block_on(collect(
        &f.provider,
        &query(),
        &f.scope,
        &f.requester,
        &f.config,
    ))
    .unwrap();
    assert!(changed.reindex_required);
    assert!(changed.candidates.is_empty());
}
