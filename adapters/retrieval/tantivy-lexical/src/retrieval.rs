//! Lexical candidate retrieval over Tantivy BM25 hits.
//!
//! This module adapts ranked `(target_id, score)` hits into portable retrieval
//! candidates. Target rehydration (canonical chunk content, provenance, policy)
//! is injected through [`LexicalTargetResolver`], so the Tantivy index stays
//! secondary adapter state rather than domain truth — mirroring the sqlite-vec
//! vector adapter.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    FusionStrategy, FusionTrace, Metadata, Policy, Provenance, RerankStrategy,
    RetrievalExplanation, RetrievalRequest, RetrievalResult, RetrievalScore, RetrievalTargetType,
};
use engram_retrieval::RetrievalIndex;
use engram_runtime::{CoreError, CoreResult};

use crate::LexicalIndex;

/// Rehydrates a lexical hit into a portable retrieval target.
///
/// Resolvers own canonical record lookup and policy-aware target visibility.
/// Returning `Ok(None)` means the indexed target is stale or not visible for
/// this request and should be skipped.
pub trait LexicalTargetResolver: Send + Sync {
    /// Resolves one BM25 hit's target id into a retrieval target.
    fn resolve(
        &self,
        target_id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<LexicalResolvedTarget>>;
}

/// Canonical target data required before a lexical hit can become a result.
#[derive(Debug, Clone, PartialEq)]
pub struct LexicalResolvedTarget {
    pub target_type: RetrievalTargetType,
    pub target_id: String,
    pub content: String,
    pub provenance: Provenance,
    pub policy: Policy,
    pub explanation: Option<RetrievalExplanation>,
    pub metadata: Option<Metadata>,
}

/// Retrieval candidate source backed by Tantivy BM25 search over chunk text.
pub struct LexicalRetrievalIndex {
    index: Arc<LexicalIndex>,
    target_resolver: Arc<dyn LexicalTargetResolver>,
    default_limit: u32,
}

impl LexicalRetrievalIndex {
    /// Creates a lexical retrieval index with the default candidate limit.
    pub fn new(index: LexicalIndex, target_resolver: Arc<dyn LexicalTargetResolver>) -> Self {
        Self::with_default_limit(index, target_resolver, 20)
    }

    /// Creates a lexical retrieval index with an explicit fallback limit.
    pub fn with_default_limit(
        index: LexicalIndex,
        target_resolver: Arc<dyn LexicalTargetResolver>,
        default_limit: u32,
    ) -> Self {
        Self {
            index: Arc::new(index),
            target_resolver,
            default_limit,
        }
    }
}

#[async_trait]
impl RetrievalIndex for LexicalRetrievalIndex {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = search_limit(request, self.default_limit) as usize;
        let hits = self
            .index
            .search(&request.query, limit)
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-lexical".to_owned(),
                message: e.to_string(),
            })?;

        let mut results = Vec::with_capacity(hits.len());
        for (rank, (target_id, score)) in hits.into_iter().enumerate() {
            let Some(target) = self.target_resolver.resolve(&target_id, request)? else {
                continue;
            };
            results.push(lexical_result(rank, &target_id, score, target));
        }
        Ok(results)
    }
}

fn search_limit(request: &RetrievalRequest, default_limit: u32) -> u32 {
    request
        .limit
        .or_else(|| request.budget.as_ref().and_then(|budget| budget.max_items))
        .unwrap_or(default_limit)
}

fn lexical_result(
    rank: usize,
    target_id: &str,
    score: f32,
    target: LexicalResolvedTarget,
) -> RetrievalResult {
    RetrievalResult {
        id: format!("lexical-result-{target_id}"),
        target_type: target.target_type,
        target_id: target.target_id,
        content: target.content,
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
            recency: None,
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: target.provenance,
        policy: target.policy,
        explanation: target.explanation,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: "lexical.keyword".to_owned(),
            source_rank: Some((rank + 1) as u32),
            source_score: Some(score),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(score),
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: target.metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use engram_domain::*;
    use futures::executor::block_on;
    use std::collections::BTreeMap;

    #[test]
    fn lexical_retrieval_returns_bm25_ranked_candidates() {
        let index = LexicalIndex::new().unwrap();
        index.upsert("chunk-a", "parse parse parse").unwrap();
        index.upsert("chunk-b", "parse").unwrap();

        let retrieval = LexicalRetrievalIndex::new(
            index,
            Arc::new(TargetMap {
                targets: BTreeMap::from([
                    ("chunk-a".to_owned(), resolved_chunk("chunk-a")),
                    ("chunk-b".to_owned(), resolved_chunk("chunk-b")),
                ]),
            }),
        );

        let results = block_on(retrieval.retrieve_candidates(&request("parse", Some(2))))
            .expect("lexical candidates");

        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert_eq!(ids, vec!["chunk-a", "chunk-b"]);
        assert_eq!(results[0].target_type, RetrievalTargetType::Chunk);
        assert!(results[0].score.total > results[1].score.total);

        let trace = results[0].fusion_trace.as_ref().expect("fusion trace");
        assert_eq!(trace.source, "lexical.keyword");
        assert_eq!(trace.source_rank, Some(1));
        assert_eq!(trace.fusion_strategy, Some(FusionStrategy::None));
    }

    #[test]
    fn lexical_retrieval_skips_missing_targets() {
        let index = LexicalIndex::new().unwrap();
        index.upsert("chunk-a", "parse parse parse").unwrap();
        index.upsert("chunk-b", "parse").unwrap();

        let retrieval = LexicalRetrievalIndex::new(
            index,
            Arc::new(TargetMap {
                targets: BTreeMap::from([("chunk-b".to_owned(), resolved_chunk("chunk-b"))]),
            }),
        );

        let results = block_on(retrieval.retrieve_candidates(&request("parse", Some(2))))
            .expect("lexical candidates");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].target_id, "chunk-b");
    }

    struct TargetMap {
        targets: BTreeMap<String, LexicalResolvedTarget>,
    }

    impl LexicalTargetResolver for TargetMap {
        fn resolve(
            &self,
            target_id: &str,
            _request: &RetrievalRequest,
        ) -> CoreResult<Option<LexicalResolvedTarget>> {
            Ok(self.targets.get(target_id).cloned())
        }
    }

    fn request(query: &str, limit: Option<u32>) -> RetrievalRequest {
        RetrievalRequest {
            query: query.to_owned(),
            scope: scope(),
            requester: requester(),
            modes: vec![RetrievalMode::Keyword],
            filters: None,
            cues: Vec::new(),
            limit,
            budget: None,
            include_explanations: Some(true),
        }
    }

    fn resolved_chunk(id: &str) -> LexicalResolvedTarget {
        LexicalResolvedTarget {
            target_type: RetrievalTargetType::Chunk,
            target_id: id.to_owned(),
            content: format!("content for {id}"),
            provenance: provenance(),
            policy: policy(),
            explanation: None,
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
            source: "lexical_retrieval_test".to_owned(),
            actor: requester().actor,
            observed_at: Utc
                .with_ymd_and_hms(2026, 7, 8, 12, 0, 0)
                .single()
                .expect("fixed timestamp"),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        }
    }
}
