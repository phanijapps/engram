//! MMR selection algorithm + `RetrievalReranker` impl.
//!
//! Given fused candidates with relevance scores and (optional) content
//! embeddings, MMR iteratively selects candidates that balance relevance
//! against diversity from already-selected items. The classic formulation
//! (ported from zbot's `gateway-memory/src/recall/mmr.rs`):
//!
//! ```text
//! next = argmax over remaining of:
//!     lambda * relevance(c) - (1 - lambda) * max(sim(c, s) for s in Selected)
//! ```
//!
//! - `lambda = 1.0` → pure relevance ordering (stable sort by score).
//! - `lambda = 0.0` → pure diversity (selects most-different remaining).
//! - `lambda = 0.5` (default from `RerankConfig`) → balanced.
//!
//! Candidates with no embedding (embedder absent or embedding failed) degrade
//! gracefully: their diversity penalty is `0.0` (treated as orthogonal to
//! everything), so their relevance still applies and they remain selectable —
//! never a panic.

use std::sync::Arc;

use engram_domain::{
    FusionStrategy, FusionTrace, RerankStrategy, RetrievalRequest, RetrievalResult,
};
use engram_retrieval::RetrievalReranker;
use engram_runtime::CoreResult;

/// Injected embedder for candidate-content text. MMR embeds each candidate's
/// `content` to compute pairwise cosine similarity; the diversity term is the
/// max cosine similarity to already-selected candidates.
///
/// This is a local trait by design (see crate docs): the integration facade's
/// `EmbeddingProvider` cannot be named here without a package cycle, so the
/// bootstrap provides a bridge impl behind the `fastembed` feature.
pub trait MmrEmbedder: Send + Sync {
    /// Embeds `text`, returning its vector. Implementations should be
    /// deterministic for a given input.
    fn embed(&self, text: &str) -> CoreResult<Vec<f32>>;
}

/// MMR relevance/diversity reranker.
///
/// `lambda = 1` ⇒ pure relevance (stable relevance sort); `lambda = 0` ⇒ max
/// diversity. Construct with [`Self::new`], passing the embedder (or `None` to
/// degrade to relevance-only) and `lambda` from `RerankConfig`.
pub struct MmrReranker {
    embedder: Option<Arc<dyn MmrEmbedder>>,
    lambda: f64,
}

impl MmrReranker {
    /// Creates an MMR reranker.
    ///
    /// `embedder = None` ⇒ graceful relevance-only degradation (no diversity
    /// signal). `lambda` is clamped to `[0, 1]` so a misconfigured value can
    /// never push the trade-off out of range.
    pub fn new(embedder: Option<Arc<dyn MmrEmbedder>>, lambda: f32) -> Self {
        Self {
            embedder,
            lambda: clamp01(lambda as f64),
        }
    }
}

impl RetrievalReranker for MmrReranker {
    fn rerank(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>> {
        if candidates.len() <= 1 {
            return Ok(candidates);
        }

        // Embed each candidate; `None` when there is no embedder or embedding
        // fails (graceful — the candidate stays, just contributes no diversity
        // signal, matching zbot's mmr.rs None-embedding handling).
        let embeddings: Vec<Option<Vec<f32>>> = match &self.embedder {
            Some(embedder) => candidates
                .iter()
                .map(|c| embedder.embed(&c.content).ok())
                .collect(),
            None => vec![None; candidates.len()],
        };

        // Post-fusion relevance (zbot's `item.score`). Normalized per batch so
        // `lambda` holds the same relevance/diversity trade-off regardless of
        // score representation (RRF ~0.01, cosine in [0,1], etc.).
        let relevance: Vec<f64> = candidates.iter().map(relevance_of).collect();
        let relevance_scale = max_positive(&relevance);

        // Select: (candidate_index, mmr_score) pairs in selection order.
        let order = if self.lambda >= 1.0 {
            // Pure relevance: stable sort by normalized relevance desc, input
            // order on ties. Scores are the normalized relevance.
            stable_relevance_order(&relevance, relevance_scale)
        } else {
            mmr_select(&embeddings, &relevance, relevance_scale, self.lambda)
        };

        // Rebuild the output in MMR-selected order (no clones: take from a
        // `Option`-pool indexed by the selection order), then stamp each trace.
        let mut pool: Vec<Option<RetrievalResult>> = candidates.into_iter().map(Some).collect();
        let mut out: Vec<RetrievalResult> = Vec::with_capacity(order.len());
        for (rank, &(idx, score)) in order.iter().enumerate() {
            if let Some(mut result) = pool[idx].take() {
                stamp_mmr(&mut result, score, rank);
                out.push(result);
            }
        }

        // Truncate to the request budget (MMR-selected top-K), mirroring the
        // cross-encoder adapter. When `limit` is `None`, return every candidate
        // in MMR order.
        if let Some(limit) = request.limit.map(|l| l as usize) {
            out.truncate(limit);
        }
        Ok(out)
    }
}

/// Relevance carried into MMR. Uses the post-fusion `score.total` (the fused
/// relevance entering rerank). Non-finite scores collapse to `0.0` so `NaN`
/// can never poison the MMR score.
fn relevance_of(result: &RetrievalResult) -> f64 {
    let s = result.score.total as f64;
    if s.is_finite() { s } else { 0.0 }
}

/// Max finite, positive relevance, or `0.0` when none is positive. Used as the
/// per-batch relevance normalization scale.
fn max_positive(relevance: &[f64]) -> f64 {
    relevance
        .iter()
        .copied()
        .filter(|s| s.is_finite() && *s > 0.0)
        .fold(0.0_f64, f64::max)
}

/// Stable pure-relevance selection: `(index, normalized_relevance)` pairs,
/// sorted by normalized relevance descending, ties broken by input order.
fn stable_relevance_order(relevance: &[f64], scale: f64) -> Vec<(usize, f64)> {
    let mut indexed: Vec<(usize, f64)> = relevance
        .iter()
        .enumerate()
        .map(|(i, &r)| {
            let norm = if scale > 0.0 { r / scale } else { 0.0 };
            (i, norm)
        })
        .collect();
    indexed.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    indexed
}

/// MMR iterative selection. Returns `(index, mmr_score)` pairs in selection
/// order. The first pick is pure argmax over `lambda * relevance` (diversity
/// term is `0.0` while `Selected` is empty); subsequent picks apply the full
/// formula. Ties on the MMR score are broken by input order (first encountered
/// wins) for deterministic, stable output.
fn mmr_select(
    embeddings: &[Option<Vec<f32>>],
    relevance: &[f64],
    relevance_scale: f64,
    lambda: f64,
) -> Vec<(usize, f64)> {
    let n = relevance.len();
    let mut selected: Vec<usize> = Vec::with_capacity(n);
    let mut chosen = vec![false; n];
    let mut order: Vec<(usize, f64)> = Vec::with_capacity(n);

    while selected.len() < n {
        let mut best_idx: Option<usize> = None;
        let mut best_score: f64 = f64::NEG_INFINITY;

        for i in 0..n {
            if chosen[i] {
                continue;
            }
            let rel = if relevance_scale > 0.0 {
                relevance[i] / relevance_scale
            } else {
                0.0
            };
            let diversity_penalty = if selected.is_empty() {
                0.0
            } else {
                max_similarity_to_selected(&embeddings[i], embeddings, &selected)
            };
            let score = lambda * rel - (1.0 - lambda) * diversity_penalty;

            // Strictly greater — first encountered wins ties for input-order
            // stability.
            if score > best_score {
                best_score = score;
                best_idx = Some(i);
            }
        }

        match best_idx {
            Some(i) => {
                chosen[i] = true;
                selected.push(i);
                order.push((i, best_score));
            }
            // Defensive: all candidates exhausted — shouldn't trigger given the
            // `while` condition, but stay panic-free.
            None => break,
        }
    }

    order
}

/// Maximum cosine similarity between `candidate_emb` and any selected
/// candidate's embedding. Returns `0.0` when:
/// - `candidate_emb` is `None` (no embedding — treated as orthogonal),
/// - every selected item has `None` embedding (no defined similarity),
/// - all similarities are non-positive (clamped from below at `0.0` so the
///   diversity penalty never goes negative).
fn max_similarity_to_selected(
    candidate_emb: &Option<Vec<f32>>,
    embeddings: &[Option<Vec<f32>>],
    selected: &[usize],
) -> f64 {
    let Some(cand) = candidate_emb.as_deref() else {
        return 0.0;
    };
    let mut max_sim = 0.0_f64;
    for &sel_idx in selected {
        if let Some(sel) = embeddings[sel_idx].as_deref() {
            let sim = cosine_similarity(cand, sel);
            if sim > max_sim {
                max_sim = sim;
            }
        }
    }
    max_sim
}

/// Cosine similarity in `f64` between two equal-length embeddings.
///
/// Returns `0.0` for empty, mismatched-length, or zero-magnitude inputs so
/// callers never propagate `NaN` into the MMR score.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut na = 0.0_f64;
    let mut nb = 0.0_f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as f64;
        let y = *y as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Clamps `x` to `[0, 1]`.
fn clamp01(x: f64) -> f64 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

/// Stamps a result's `FusionTrace` with the MMR rerank strategy + the MMR score
/// (the per-batch-normalized score produced by selection), creating a minimal
/// trace when none was present. Provenance, policy, and target identity are
/// preserved. Mirrors the cross-encoder adapter's `stamp_rerank`.
fn stamp_mmr(result: &mut RetrievalResult, mmr_score: f64, rank: usize) {
    let mut trace = result.fusion_trace.take().unwrap_or_else(|| FusionTrace {
        query_id: None,
        vector_index: None,
        embedding_time_ms: None,
        search_time_ms: None,
        source: "rerank.mmr".to_owned(),
        source_rank: None,
        source_score: None,
        score: None,
        rank: None,
        fusion_strategy: Some(FusionStrategy::None),
        fusion_score: None,
        rerank_strategy: Some(RerankStrategy::Mmr),
        rerank_score: Some(mmr_score as f32),
        discard_reason: None,
        deduplicated_with: Vec::new(),
    });
    trace.rerank_strategy = Some(RerankStrategy::Mmr);
    trace.rerank_score = Some(mmr_score as f32);
    // rank is the 0-based MMR selection position; store as 1-based final rank.
    trace.rank = Some(rank as u32 + 1);
    result.fusion_trace = Some(trace);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use engram_domain::{
        Actor, ActorKind, AllowedUse, DeleteMode, Id, Policy, Provenance, Requester, Retention,
        RetrievalMode, RetrievalRequest, RetrievalScore, RetrievalTargetType, Scope, Sensitivity,
        Visibility,
    };
    use std::collections::HashMap;

    /// Deterministic stub embedder: maps content → fixed vector. Content not in
    /// the map embeds to a zero vector (orthogonal to everything).
    struct StubEmbedder {
        map: HashMap<String, Vec<f32>>,
    }

    impl MmrEmbedder for StubEmbedder {
        fn embed(&self, text: &str) -> CoreResult<Vec<f32>> {
            Ok(self
                .map
                .get(text)
                .cloned()
                .unwrap_or_else(|| vec![0.0, 0.0]))
        }
    }

    /// Embedder that always errors — exercises graceful None-embedding decay.
    struct FailingEmbedder;
    impl MmrEmbedder for FailingEmbedder {
        fn embed(&self, _text: &str) -> CoreResult<Vec<f32>> {
            Err(engram_runtime::CoreError::InvalidRequest {
                reason: "embedder unavailable".to_owned(),
            })
        }
    }

    fn request() -> RetrievalRequest {
        RetrievalRequest {
            query: "query".to_owned(),
            scope: Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            requester: Requester {
                actor: Actor {
                    id: Id::from("actor-test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            modes: vec![RetrievalMode::Semantic],
            filters: None,
            cues: Vec::new(),
            limit: None,
            budget: None,
            include_explanations: None,
        }
    }

    fn candidate(id: &str, content: &str, relevance_total: f32) -> RetrievalResult {
        RetrievalResult {
            id: format!("result-{id}"),
            target_type: RetrievalTargetType::Chunk,
            target_id: id.to_owned(),
            content: content.to_owned(),
            score: RetrievalScore {
                total: relevance_total,
                relevance: Some(relevance_total),
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: Some(1.0),
            },
            provenance: Provenance {
                source: "mmr_test".to_owned(),
                actor: Actor {
                    id: Id::from("actor-test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: Utc
                    .with_ymd_and_hms(2026, 7, 8, 12, 0, 0)
                    .single()
                    .expect("fixed timestamp"),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("test".to_owned()),
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: Some(Sensitivity::Low),
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: Some(DeleteMode::Tombstone),
            },
            explanation: None,
            fusion_trace: None,
            metadata: None,
        }
    }

    fn ids(results: &[RetrievalResult]) -> Vec<&str> {
        results.iter().map(|r| r.target_id.as_str()).collect()
    }

    #[test]
    fn mmr_demotes_near_duplicate_vs_pure_relevance() {
        // A and B are near-duplicate (cosine ≈ 1); C is orthogonal to A.
        let mut map = HashMap::new();
        map.insert("alpha".to_owned(), vec![1.0, 0.0]);
        map.insert("alpha dup".to_owned(), vec![0.99, 0.01]);
        map.insert("beta".to_owned(), vec![0.0, 1.0]);
        let embedder: Arc<dyn MmrEmbedder> = Arc::new(StubEmbedder { map });

        // Relevances: A (0.9) > B (0.8) > C (0.5).
        let cands = vec![
            candidate("a", "alpha", 0.9),
            candidate("b", "alpha dup", 0.8),
            candidate("c", "beta", 0.5),
        ];
        let req = request();

        // MMR (lambda=0.5): A first; B demoted below C — B≈A is heavily
        // penalized while C is orthogonal (no penalty).
        let mmr = MmrReranker::new(Some(embedder), 0.5);
        let out = mmr.rerank(&req, cands).expect("mmr rerank");
        assert_eq!(
            ids(&out),
            vec!["a", "c", "b"],
            "MMR should demote the near-duplicate (b) below the diverse lower-relevance (c)"
        );

        // Every result carries the MMR rerank stamp.
        for r in &out {
            let trace = r.fusion_trace.as_ref().expect("trace");
            assert_eq!(trace.rerank_strategy, Some(RerankStrategy::Mmr));
        }
    }

    #[test]
    fn lambda_one_is_pure_relevance_order() {
        let mut map = HashMap::new();
        map.insert("alpha".to_owned(), vec![1.0, 0.0]);
        map.insert("alpha dup".to_owned(), vec![0.99, 0.01]);
        map.insert("beta".to_owned(), vec![0.0, 1.0]);
        let embedder: Arc<dyn MmrEmbedder> = Arc::new(StubEmbedder { map });

        let cands = vec![
            candidate("a", "alpha", 0.9),
            candidate("b", "alpha dup", 0.8),
            candidate("c", "beta", 0.5),
        ];
        let req = request();

        // lambda = 1.0 ⇒ pure relevance: A, B, C (near-duplicate NOT demoted).
        let relevance_only = MmrReranker::new(Some(embedder.clone()), 1.0);
        let out = relevance_only.rerank(&req, cands.clone()).expect("rerank");
        assert_eq!(ids(&out), vec!["a", "b", "c"]);

        // A mid lambda must differ from pure relevance on the same set,
        // proving the trade-off knob does something.
        let balanced = MmrReranker::new(Some(embedder), 0.5);
        let out_balanced = balanced.rerank(&req, cands).expect("rerank");
        assert_ne!(ids(&out), ids(&out_balanced));
    }

    #[test]
    fn graceful_when_embedder_or_embedding_missing() {
        let cands = vec![
            candidate("a", "alpha", 0.9),
            candidate("b", "beta", 0.5),
            candidate("c", "gamma", 0.2),
        ];
        let req = request();

        // No embedder at all: degrade to relevance order, no panic.
        let no_embedder = MmrReranker::new(None, 0.5);
        let out = no_embedder.rerank(&req, cands.clone()).expect("rerank");
        assert_eq!(ids(&out), vec!["a", "b", "c"]);

        // Failing embedder: every embedding fails → all None → same relevance
        // ordering, no panic, no error propagated.
        let failing: Arc<dyn MmrEmbedder> = Arc::new(FailingEmbedder);
        let with_failing = MmrReranker::new(Some(failing), 0.5);
        let out = with_failing.rerank(&req, cands).expect("rerank");
        assert_eq!(ids(&out), vec!["a", "b", "c"]);
    }

    #[test]
    fn empty_and_single_pass_through_unchanged() {
        let req = request();
        let r = MmrReranker::new(None, 0.5);
        assert!(r.rerank(&req, Vec::new()).unwrap().is_empty());
        let one = vec![candidate("only", "x", 0.4)];
        let out = r.rerank(&req, one.clone()).unwrap();
        assert_eq!(ids(&out), vec!["only"]);
    }

    #[test]
    fn truncates_to_request_limit_in_mmr_order() {
        let mut map = HashMap::new();
        map.insert("alpha".to_owned(), vec![1.0, 0.0]);
        map.insert("alpha dup".to_owned(), vec![0.99, 0.01]);
        map.insert("beta".to_owned(), vec![0.0, 1.0]);
        let embedder: Arc<dyn MmrEmbedder> = Arc::new(StubEmbedder { map });

        let cands = vec![
            candidate("a", "alpha", 0.9),
            candidate("b", "alpha dup", 0.8),
            candidate("c", "beta", 0.5),
        ];
        let mut req = request();
        req.limit = Some(2);

        let mmr = MmrReranker::new(Some(embedder), 0.5);
        let out = mmr.rerank(&req, cands).expect("rerank");
        // MMR top-2 of [a, c, b] → [a, c].
        assert_eq!(ids(&out), vec!["a", "c"]);
    }
}
