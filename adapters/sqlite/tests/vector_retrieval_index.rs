use std::{collections::BTreeMap, sync::Arc};

use chrono::{TimeZone, Utc};
use engram_domain::*;
use engram_retrieval::RetrievalIndex;
use engram_runtime::CoreResult;
use engram_store_sqlite::{
    SqliteVectorIndex, VectorEntry, VectorQueryProvider, VectorResolvedTarget,
    VectorRetrievalIndex, VectorSearchResult, VectorTargetResolver,
};
use futures::executor::block_on;

#[derive(Debug)]
struct FixedQuery(Vec<f32>);

impl VectorQueryProvider for FixedQuery {
    fn query_vector(&self, _request: &RetrievalRequest) -> CoreResult<Vec<f32>> {
        Ok(self.0.clone())
    }
}

#[derive(Debug)]
struct TargetMap {
    targets: BTreeMap<String, VectorResolvedTarget>,
}

impl VectorTargetResolver for TargetMap {
    fn resolve(
        &self,
        hit: &VectorSearchResult,
        _request: &RetrievalRequest,
    ) -> CoreResult<Option<VectorResolvedTarget>> {
        Ok(self.targets.get(&hit.target_id).cloned())
    }
}

#[test]
fn vector_retrieval_returns_nearest_rehydrated_candidates() {
    let index = vector_index();
    let retrieval = VectorRetrievalIndex::new(
        index,
        Arc::new(FixedQuery(vec![1.0, 0.0, 0.0])),
        Arc::new(TargetMap {
            targets: BTreeMap::from([
                (
                    "chunk-a".to_owned(),
                    resolved_chunk("chunk-a", "near vector chunk"),
                ),
                (
                    "chunk-b".to_owned(),
                    resolved_chunk("chunk-b", "far vector chunk"),
                ),
            ]),
        }),
    );

    let results =
        block_on(retrieval.retrieve_candidates(&request(Some(2)))).expect("vector candidates");

    assert_eq!(
        results
            .iter()
            .map(|result| result.target_id.as_str())
            .collect::<Vec<_>>(),
        vec!["chunk-a", "chunk-b"]
    );
    assert_eq!(results[0].target_type, RetrievalTargetType::Chunk);
    assert!(results[0].score.total > results[1].score.total);
    let trace = results[0].fusion_trace.as_ref().expect("fusion trace");
    assert_eq!(trace.source, "vector.semantic");
    assert_eq!(trace.source_rank, Some(1));
    assert_eq!(trace.fusion_strategy, Some(FusionStrategy::None));
}

#[test]
fn vector_retrieval_skips_missing_targets() {
    let index = vector_index();
    let retrieval = VectorRetrievalIndex::new(
        index,
        Arc::new(FixedQuery(vec![1.0, 0.0, 0.0])),
        Arc::new(TargetMap {
            targets: BTreeMap::from([(
                "chunk-b".to_owned(),
                resolved_chunk("chunk-b", "only resolved chunk"),
            )]),
        }),
    );

    let results =
        block_on(retrieval.retrieve_candidates(&request(Some(2)))).expect("vector candidates");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].target_id, "chunk-b");
}

#[test]
fn vector_retrieval_reports_query_dimension_mismatch() {
    let index = vector_index();
    let retrieval = VectorRetrievalIndex::new(
        index,
        Arc::new(FixedQuery(vec![1.0, 0.0])),
        Arc::new(TargetMap {
            targets: BTreeMap::new(),
        }),
    );

    let error = block_on(retrieval.retrieve_candidates(&request(Some(2))))
        .expect_err("dimension mismatch should fail");

    assert!(error.to_string().contains("vector dimensions mismatch"));
}

fn vector_index() -> SqliteVectorIndex {
    let index = SqliteVectorIndex::open_in_memory(3).expect("open index");
    index
        .insert(VectorEntry {
            id: "embedding-a".to_owned(),
            target_type: EmbeddingTargetType::Chunk,
            target_id: "chunk-a".to_owned(),
            model: "fixture".to_owned(),
            dimensions: 3,
            content_hash: "sha256:a".to_owned(),
            embedding: vec![1.0, 0.0, 0.0],
        })
        .expect("insert near vector");
    index
        .insert(VectorEntry {
            id: "embedding-b".to_owned(),
            target_type: EmbeddingTargetType::Chunk,
            target_id: "chunk-b".to_owned(),
            model: "fixture".to_owned(),
            dimensions: 3,
            content_hash: "sha256:b".to_owned(),
            embedding: vec![0.0, 1.0, 0.0],
        })
        .expect("insert far vector");
    index
}

fn request(limit: Option<u32>) -> RetrievalRequest {
    RetrievalRequest {
        query: "semantic chunk".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Semantic],
        filters: None,
        cues: Vec::new(),
        limit,
        budget: None,
        include_explanations: Some(true),
    }
}

fn resolved_chunk(id: &str, content: &str) -> VectorResolvedTarget {
    VectorResolvedTarget {
        target_type: RetrievalTargetType::Chunk,
        target_id: id.to_owned(),
        content: content.to_owned(),
        provenance: provenance(),
        policy: policy(),
        explanation: Some(RetrievalExplanation {
            reason: "Resolved vector target fixture.".to_owned(),
            matched_cues: Vec::new(),
            matched_terms: Vec::new(),
            path: vec!["docs/reference.md".to_owned()],
            source_summary: Some("fixture".to_owned()),
        }),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: Actor {
            id: Id::from("actor-test"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        roles: Vec::new(),
        permissions: Vec::new(),
        on_behalf_of: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "vector_retrieval_test".to_owned(),
        actor: requester().actor,
        observed_at: Utc
            .with_ymd_and_hms(2026, 6, 30, 12, 0, 0)
            .single()
            .expect("fixed timestamp"),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_owned()),
    }
}
