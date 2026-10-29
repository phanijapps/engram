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
use engram_retrieval::{CodeEligibilityResolver, RetrievalIndex, resolve_view};
use engram_runtime::{CoreError, CoreResult};

use crate::LexicalIndex;

/// Rehydrates a lexical hit into a portable retrieval target.
///
/// Resolvers own canonical record lookup and policy-aware target visibility.
/// Returning `Ok(None)` means the indexed target is stale or not visible for
/// this request and should be skipped.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait LexicalTargetResolver: Send + Sync {
    /// Resolves one BM25 hit's target id into a retrieval target.
    ///
    /// Async so implementors await canonical-record lookups (e.g. the store's
    /// `get_chunk`) instead of nesting a `block_on` inside the already-async
    /// recall pipeline — which would panic (`LocalPool` re-entry).
    async fn resolve(
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
    /// True when the resolved record is a CODE KIND (symbol entity, code
    /// chunk kind). Corrective r2 (finding 4): together with the resolver's
    /// durable lineage this drives classification — an unstamped code-kind
    /// record is legacy unbound code and never eligible.
    pub code_kind: bool,
}

/// Retrieval candidate source backed by Tantivy BM25 search over chunk text.
pub struct LexicalRetrievalIndex {
    index: Arc<LexicalIndex>,
    target_resolver: Arc<dyn LexicalTargetResolver>,
    default_limit: u32,
    /// Optional bound-code eligibility: when wired, BM25 collection is
    /// constrained to the eligible contributor set (published code
    /// generations + ordinary documents) BEFORE the limit, and each resolved
    /// target is classified against the resolved view before it can enter
    /// the candidate page (REQ-005, task_013 seam). `None` keeps the
    /// historical unconstrained lane for deployments without bindings.
    eligibility: Option<Arc<dyn CodeEligibilityResolver>>,
}

impl LexicalRetrievalIndex {
    /// Creates a lexical retrieval index with the default candidate limit.
    pub fn new(index: LexicalIndex, target_resolver: Arc<dyn LexicalTargetResolver>) -> Self {
        Self::with_default_limit(index, target_resolver, 20)
    }

    /// Creates a lexical retrieval index from an already-shared index, so the
    /// same `Arc<LexicalIndex>` can feed a `LexicalFeed` (writes) and serve
    /// retrieval (reads) over one in-RAM Tantivy index.
    pub fn from_arc(
        index: Arc<LexicalIndex>,
        target_resolver: Arc<dyn LexicalTargetResolver>,
    ) -> Self {
        Self {
            index,
            target_resolver,
            default_limit: 20,
            eligibility: None,
        }
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
            eligibility: None,
        }
    }

    /// [`Self::from_arc`] with a bound-code eligibility resolver wired in:
    /// every query resolves the scope's published generations once, collects
    /// BM25 within the eligible contributor set, and classifies each resolved
    /// target before it can consume a candidate slot.
    pub fn from_arc_with_eligibility(
        index: Arc<LexicalIndex>,
        target_resolver: Arc<dyn LexicalTargetResolver>,
        eligibility: Arc<dyn CodeEligibilityResolver>,
    ) -> Self {
        Self {
            index,
            target_resolver,
            default_limit: 20,
            eligibility: Some(eligibility),
        }
    }

    /// The allowed Tantivy contributor keys for a resolved view: ordinary
    /// documents always pass; each published code generation contributes its
    /// `code:<generation>` key. Staged/retired/unbound generations are absent,
    /// so their documents cannot enter the collector at all.
    fn allowed_contributor_keys(
        view: &engram_retrieval::BoundCodeView,
    ) -> std::collections::BTreeSet<String> {
        let mut keys = std::collections::BTreeSet::new();
        keys.insert(crate::LEXICAL_DOC_CONTRIBUTOR_KEY.to_owned());
        for generation in view.eligible_generations() {
            keys.insert(crate::code_contributor_key(&generation));
        }
        keys
    }
}

#[async_trait]
impl RetrievalIndex for LexicalRetrievalIndex {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = search_limit(request, self.default_limit) as usize;
        // Resolve bound-code eligibility ONCE per request. The view drives
        // (a) the collector constraint below — before the TopDocs limit — and
        // (b) per-target classification after resolution. A resolver failure
        // fails the lane closed (it degrades via source_failures upstream),
        // never silently widens to an unconstrained page.
        let view = match &self.eligibility {
            Some(resolver) => Some(
                resolve_view(
                    resolver.as_ref(),
                    &request.scope,
                    request
                        .filters
                        .as_ref()
                        .and_then(|filters| filters.code_index_keys.as_deref()),
                )
                .await?,
            ),
            None => None,
        };
        // Code-artifact suppression runs at the RESOLVER (the only place the
        // full entity is available), which is AFTER the BM25 search below —
        // so over-fetch now (4×, floor 32) and truncate after filtering,
        // otherwise suppressed import/file stubs (short docs BM25 loves)
        // would consume the entire candidate page and starve real hits.
        let fetch = if engram_retrieval::suppresses_code_artifacts(request) {
            (limit * 4).max(32)
        } else {
            limit
        };
        let hits = match &view {
            Some(view) => {
                let keys = Self::allowed_contributor_keys(view);
                self.index
                    .search_within(&request.query, fetch, &keys)
                    .map_err(|e| CoreError::Adapter {
                        adapter: "engram-store-lexical".to_owned(),
                        message: e.to_string(),
                    })?
            }
            None => self
                .index
                .search(&request.query, fetch)
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-lexical".to_owned(),
                    message: e.to_string(),
                })?,
        };

        let mut results = Vec::with_capacity(hits.len());
        for (rank, (target_id, score)) in hits.into_iter().enumerate() {
            let Some(target) = self.target_resolver.resolve(&target_id, request).await? else {
                continue;
            };
            // Resolver seam: suppressed entity targets resolve to None (see
            // KnowledgeLexicalResolver), keeping the eligibility decision in
            // one place; the over-fetch above refills the page.
            //
            // Bound-code classification (corrective r2, finding 4): the
            // durable-lineage view + the resolver's CODE KIND decide — a
            // lineage record on a non-published binding, or a code-kind
            // record without lineage (legacy unbound code), drops BEFORE the
            // page budget is consumed; ordinary non-code targets always pass
            // (AC-006).
            if let Some(view) = &view
                && !view.allows(target.target_id.as_str(), target.code_kind)
            {
                continue;
            }
            results.push(lexical_result(rank, &target_id, score, target));
        }
        results.truncate(limit);
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
            source: "lexical".to_owned(),
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
        assert_eq!(trace.source, "lexical");
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

    #[async_trait]
    impl LexicalTargetResolver for TargetMap {
        async fn resolve(
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
            code_kind: false,
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
