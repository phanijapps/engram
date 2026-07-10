//! Backend-neutral unified recall port (engram-host-sdk brief, S4).
//!
//! [`UnifiedRecall`] fans one recall query across the available semantic lanes —
//! facts (memory), graph/vector/lexical (via `RetrievalIndex` lanes), and beliefs
//! (via `BeliefRepository`) — and fuses them through the existing
//! `ReciprocalRankFusion` + `compose_context` into one [`ContextPayload`]. The
//! port reuses the existing retrieval types; no new result type is introduced.
//!
//! v1 lanes are facts, graph, vector, lexical, and beliefs. Taxonomy-expanded
//! terms (no `expand_terms` port exists) and episodes/evidence (a provenance read
//! of a different shape) are deferred.
//!
//! ADR-0022: this port is engine-neutral — it names no engine type and holds no
//! SQL (enforced by `.codex/hooks/check-engine-neutrality.sh`). The SQLite
//! implementation lives in the adapters layer (`engram-conformance`).

use async_trait::async_trait;
use engram_domain::{ContextPayload, RetrievalRequest};
use engram_runtime::CoreResult;

/// Unified recall port: one query fanned across the v1 lanes (facts, graph,
/// vector, lexical, beliefs) and fused into a single [`ContextPayload`].
///
/// The implementation composes the existing `MemoryService::retrieve` (facts
/// lane), `RetrievalIndex` lanes (graph/vector/lexical), and
/// `BeliefRepository::get_belief` (beliefs lane — at most one belief per query),
/// then fuses via `ReciprocalRankFusion` + `compose_context`. On a lane error, a
/// `RetrievalSourceFailure` is recorded and the recall continues (degraded, not
/// aborted). All lanes failing still returns `Ok` with an empty items list and
/// one `RetrievalSourceFailure` per lane — degraded success, never `Err`.
///
/// # v1 limitations
///
/// The **beliefs lane** matches when `request.query` exactly equals a belief's
/// `subject.key` (`BeliefQuery::live_subject` does an exact-match lookup).
/// Free-text queries that do not verbatim match a stored subject key will yield
/// `None` from this lane — fuzzy / entity-alias mapping is a follow-up (see
/// `docs/backlog.md`, `unified-recall-taxonomy-episodes`).
#[async_trait]
pub trait UnifiedRecall: Send + Sync {
    /// Fans `request` across the v1 lanes and returns one fused [`ContextPayload`].
    async fn recall(&self, request: RetrievalRequest) -> CoreResult<ContextPayload>;
}
