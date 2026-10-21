//! Temporal retrieval lane — recency-weighted memory candidates.
//!
//! One of the durable `RetrievalMode` lanes that were missing after the
//! process-local fixture retired (retrieval-completeness Phase 2). Mirrors
//! `GraphRetrievalIndex`: an engine-specific `RetrievalIndex` that holds a
//! `TemporalMemorySource` (the concrete `SqlMemoryService` impls it), lists
//! in-scope memories, and ranks them by an exponential recency decay. The
//! resulting candidates fuse with the other lanes via the existing RRF +
//! `compose_context`, giving recall a *recency* signal it previously lacked.
//!
//! Engine-specific (it names `Sql*`), so it lives here in `engram_store_sqlite`
//! behind the storage-neutral `RetrievalIndex` port — the same exemption
//! `GraphRetrievalIndex` uses (ADR-0022).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use engram_domain::{
    FusionStrategy, FusionTrace, MemoryRecord, MemoryStatus, RerankStrategy, RetrievalRequest,
    RetrievalResult, RetrievalScore, RetrievalTargetType, Scope,
};
use engram_retrieval::RetrievalIndex;
use engram_runtime::CoreResult;

use crate::memory::SqlMemoryService;

/// Source of recent in-scope memories for the temporal lane. `SqlMemoryService`
/// implements it; tests can stub it.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait TemporalMemorySource: Send + Sync {
    /// Recent active memories visible to `scope`, newest-first, capped at `limit`.
    async fn recent_memories(&self, scope: &Scope, limit: u32) -> CoreResult<Vec<MemoryRecord>>;
}

/// Default half-life for the recency decay (days). A memory this old scores 0.5.
const DEFAULT_HALF_LIFE_DAYS: f64 = 14.0;

/// Exponential recency decay in `(0.0, 1.0]`: 1.0 at age 0, 0.5 at one half-life,
/// halving each half-life thereafter. Pure + unit-tested.
pub fn recency_score(created_at: DateTime<Utc>, now: DateTime<Utc>, half_life_days: f64) -> f32 {
    let age_secs = (now - created_at).num_seconds().max(0) as f64;
    let age_days = age_secs / 86_400.0;
    0.5_f32.powf((age_days / half_life_days.max(1e-9)) as f32)
}

/// The temporal retrieval lane.
pub struct TemporalRetrievalIndex {
    source: Arc<dyn TemporalMemorySource>,
    default_limit: u32,
    half_life_days: f64,
}

impl TemporalRetrievalIndex {
    /// Creates a temporal lane with the default candidate limit (20) + half-life.
    pub fn new(source: Arc<dyn TemporalMemorySource>) -> Self {
        Self::with_default_limit(source, 20)
    }

    /// Creates a temporal lane with an explicit fallback limit + default half-life.
    pub fn with_default_limit(source: Arc<dyn TemporalMemorySource>, default_limit: u32) -> Self {
        Self {
            source,
            default_limit,
            half_life_days: DEFAULT_HALF_LIFE_DAYS,
        }
    }
}

#[async_trait]
impl RetrievalIndex for TemporalRetrievalIndex {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = request
            .limit
            .or_else(|| request.budget.as_ref().and_then(|budget| budget.max_items))
            .unwrap_or(self.default_limit);
        let now = Utc::now();
        // Overfetch so the query gate can discard unrelated recents without
        // starving the lane (recent N x 5, then gate, then rank + cap).
        let fetch = limit.saturating_mul(5).max(limit);
        let records = self.source.recent_memories(&request.scope, fetch).await?;
        let gated = query_gate(request.query.as_str(), records);
        Ok(rank_temporal(gated, now, self.half_life_days, limit))
    }
}

/// RELEVANCE GATE (memory-recall main defect): the temporal lane used to
/// return the newest memories regardless of the question — a just-written
/// memory scored ~1.0 for ANY query and, through RRF, covered the right
/// answer in every recall. Recency must AMPLIFY relevant memories, never
/// substitute for relevance: a candidate survives only when its text
/// contains at least one meaningful query token (shared heuristic policy:
/// function words + 1-char tokens dropped). A query with no surviving
/// tokens contributes NOTHING from this lane — correct, an unrelated
/// question should not surface recent memories at all.
fn query_gate(query: &str, records: Vec<MemoryRecord>) -> Vec<MemoryRecord> {
    let tokens = engram_retrieval::query_tokens(query);
    if tokens.is_empty() {
        return Vec::new();
    }
    records
        .into_iter()
        .filter(|r| {
            let lower = r.content.text.to_lowercase();
            tokens.iter().any(|t| lower.contains(t.as_str()))
        })
        .collect()
}

/// Pure: score memories by recency, sort newest-first, cap at `limit`, and build
/// `RetrievalResult`s with `score.recency` set. The recency score IS the total
/// (this lane ranks purely by freshness; RRF blends it with relevance lanes).
pub fn rank_temporal(
    records: Vec<MemoryRecord>,
    now: DateTime<Utc>,
    half_life_days: f64,
    limit: u32,
) -> Vec<RetrievalResult> {
    let mut scored: Vec<(f32, MemoryRecord)> = records
        .into_iter()
        .map(|r| (recency_score(r.created_at, now, half_life_days), r))
        .collect();
    // Newest (highest recency) first.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(limit.max(1) as usize)
        .map(|(score, record)| memory_result(&record, score))
        .collect()
}

/// Build a `RetrievalResult` for a recency-scored memory. Mirrors the shape used
/// by the memory retrieval path (target_type Memory, content text).
fn memory_result(record: &MemoryRecord, score: f32) -> RetrievalResult {
    RetrievalResult {
        id: format!("result-{}", record.id),
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        content: record.content.text.clone(),
        score: RetrievalScore {
            total: score,
            relevance: None,
            recency: Some(score),
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: record.provenance.clone(),
        policy: record.policy.clone(),
        explanation: None,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: "temporal".to_owned(),
            source_rank: None,
            source_score: Some(score),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: None,
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

/// `SqlMemoryService` is the temporal lane's source: in-scope memories, filtered
/// to active, newest-first, capped at the requested limit.
#[async_trait]
impl TemporalMemorySource for SqlMemoryService {
    async fn recent_memories(&self, scope: &Scope, limit: u32) -> CoreResult<Vec<MemoryRecord>> {
        let mut records = self.list_memories_in_scope(scope)?;
        records.retain(|r| r.status == MemoryStatus::Active);
        records.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        records.truncate(limit.max(1) as usize);
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The memory-recall main defect: the newest memory scored ~1.0 for ANY
    /// query and covered the right answer in every recall question. The
    /// gate must drop recents that share no meaningful token with the query.
    #[test]
    fn query_gate_drops_unrelated_recents() {
        let now = Utc::now();
        let records = vec![
            mem("Deployed the payment gateway to staging today", now, 1),
            mem(
                "The capital of France is Paris",
                now - chrono::Duration::days(1),
                2,
            ),
        ];
        let gated = query_gate("what is the capital of france", records);
        assert_eq!(gated.len(), 1, "only the relevant memory survives");
        assert!(gated[0].content.text.contains("Paris"));
    }

    /// Recency still ranks among RELEVANT candidates (recent-and-relevant
    /// first, old-and-relevant still present) — the gate must not become a
    /// "recent only" filter.
    #[test]
    fn query_gate_keeps_old_relevant_memories() {
        let now = Utc::now();
        let records = vec![
            mem("Retries use exponential backoff, newest note", now, 1),
            mem(
                "Retries use exponential backoff, older note",
                now - chrono::Duration::days(30),
                2,
            ),
        ];
        let gated = query_gate("retry backoff policy", records);
        assert_eq!(gated.len(), 2, "both relevant memories survive");
    }

    /// Stopword-only queries contribute NOTHING from this lane (never the
    /// newest memories regardless of question).
    #[test]
    fn query_gate_empty_tokens_contribute_nothing() {
        let records = vec![mem("anything at all", Utc::now(), 1)];
        assert!(query_gate("how to the", records.clone()).is_empty());
        assert!(query_gate("", records).is_empty());
    }

    use engram_domain::MemoryRecord;
    use engram_domain::{
        Actor, ActorKind, AllowedUse, DeleteMode, Id, MemoryContent, MemoryContentFormat,
        MemoryKind, Policy, Provenance, Sensitivity, Visibility,
    };
    use engram_domain::{Retention, Scope};
    use std::sync::Mutex;

    fn scope() -> Scope {
        Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn mem(text: &str, created_at: DateTime<Utc>, n: u8) -> MemoryRecord {
        MemoryRecord {
            id: Id::from(format!("m{n}")),
            kind: MemoryKind::Fact,
            content: MemoryContent {
                text: text.to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: Some(MemoryContentFormat::Text),
                structured: None,
                hash: None,
            },
            scope: scope(),
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: Id::from("a"),
                    kind: ActorKind::System,
                    display_name: None,
                    metadata: None,
                },
                observed_at: created_at,
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: None,
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: Some(Sensitivity::Low),
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: Some(DeleteMode::Tombstone),
            },
            status: MemoryStatus::Active,
            links: Vec::new(),
            assertions: Vec::new(),
            created_at,
            updated_at: None,
            metadata: None,
        }
    }

    #[test]
    fn recency_is_one_at_age_zero() {
        let now = Utc::now();
        assert!((recency_score(now, now, 14.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn recency_halves_per_half_life() {
        let now = Utc::now();
        let old = now - chrono::Duration::days(14);
        assert!((recency_score(old, now, 14.0) - 0.5).abs() < 1e-3);
        let older = now - chrono::Duration::days(28);
        assert!((recency_score(older, now, 14.0) - 0.25).abs() < 1e-3);
    }

    #[test]
    fn rank_orders_newest_first_and_caps() {
        let now = Utc::now();
        let records = vec![
            mem("old", now - chrono::Duration::days(10), 1),
            mem("new", now - chrono::Duration::days(1), 2),
            mem("mid", now - chrono::Duration::days(5), 3),
        ];
        let out = rank_temporal(records, now, 14.0, 2);
        assert_eq!(out.len(), 2, "capped at limit");
        assert_eq!(out[0].content, "new");
        assert_eq!(out[1].content, "mid");
        // recency score populated + monotonic.
        assert!(out[0].score.recency.unwrap() > out[1].score.recency.unwrap());
    }

    // A stub source to exercise the index end-to-end without a DB.
    #[async_trait]
    impl TemporalMemorySource for Mutex<Vec<MemoryRecord>> {
        async fn recent_memories(
            &self,
            _scope: &Scope,
            _limit: u32,
        ) -> CoreResult<Vec<MemoryRecord>> {
            Ok(self.lock().unwrap().clone())
        }
    }
}
