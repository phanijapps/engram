//! Backend-neutral provenance / evidence query port (engram-host-sdk brief, S2).
//!
//! [`ProvenanceQuery`] reads the [`Provenance`] and [`EvidenceRef`] already
//! embedded in stored records, explaining *why* an entity, relationship, or
//! source exists. It is a read-only, facade-level port: `core/integration`
//! defines the contract; the SQLite implementation lives in the adapters layer
//! (`engram-conformance`). v1 serves the knowledge-graph core — entity,
//! relationship, source — by filtering the records' existing `Provenance` /
//! `evidence` fields. Other [`EvidenceTargetType`] kinds return
//! [`engram_runtime::CoreError::CapabilityUnsupported`] until their scope-safe
//! listing is wired.
//!
//! ADR-0022: this port is engine-neutral — it names no engine type and holds no
//! SQL (enforced by `.codex/hooks/check-engine-neutrality.sh`).

use async_trait::async_trait;
use engram_domain::{EvidenceRef, EvidenceTargetType, Provenance, Scope, Timestamp};
use engram_runtime::CoreResult;

/// A `[from, to)` window over `Provenance.observed_at`. `None` bounds are open.
///
/// The query time window filters on `observed_at` for every target kind; fields
/// like `valid_from` that some record types carry are read-only metadata, not
/// v1 filter fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeWindow {
    /// Inclusive lower bound on `observed_at` (`None` = unbounded past).
    pub from: Option<Timestamp>,
    /// Exclusive upper bound on `observed_at` (`None` = unbounded future).
    pub to: Option<Timestamp>,
}

impl TimeWindow {
    /// An open window (no bounds) — matches every `observed_at`.
    pub fn open() -> Self {
        Self::default()
    }

    /// Sets the inclusive lower bound.
    pub fn from(mut self, ts: Timestamp) -> Self {
        self.from = Some(ts);
        self
    }

    /// Sets the exclusive upper bound.
    pub fn to(mut self, ts: Timestamp) -> Self {
        self.to = Some(ts);
        self
    }

    /// True when `observed_at` falls within `[from, to)`.
    pub fn contains(&self, observed_at: Timestamp) -> bool {
        if self.from.is_some_and(|f| observed_at < f) {
            return false;
        }
        if self.to.is_some_and(|t| observed_at >= t) {
            return false;
        }
        true
    }
}

/// One record's provenance, located by target — the result of a scoped or
/// source-filtered query. The embedded [`Provenance`] carries its own
/// `evidence: Vec<EvidenceRef>`, so callers reach the evidence through it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvenanceEntry {
    pub target: EvidenceTargetType,
    pub target_id: String,
    pub provenance: Provenance,
}

/// Read-only provenance / evidence query port.
///
/// Every op takes a [`Scope`] (tenant/workspace/session/environment isolation);
/// the time-window ops filter on `Provenance.observed_at`. The port accepts any
/// [`EvidenceTargetType`] as a typed input; which kinds a given backend returns
/// data for (versus [`engram_runtime::CoreError::CapabilityUnsupported`]) is an
/// implementation property — the SQLite impl backs entity, relationship, and
/// source in v1.
#[async_trait]
pub trait ProvenanceQuery: Send + Sync {
    /// Provenance carried by the record `target`/`id` in `scope`, or `None` if
    /// the record has no provenance or does not exist.
    async fn provenance_for(
        &self,
        target: EvidenceTargetType,
        id: &str,
        scope: &Scope,
    ) -> CoreResult<Option<Provenance>>;

    /// Evidence links carried by the record `target`/`id` in `scope` (empty if
    /// none, never an error for a record that simply carries no evidence).
    async fn evidence_for(
        &self,
        target: EvidenceTargetType,
        id: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<EvidenceRef>>;

    /// Provenance of records grouped under `stable_source_key` in `scope` within
    /// `window`. `stable_source_key` is the source-grouping key (typically the
    /// source URI / repo URL supplied at ingest), **not** the `KnowledgeSource.id`.
    async fn provenance_by_source(
        &self,
        stable_source_key: &str,
        scope: &Scope,
        window: TimeWindow,
    ) -> CoreResult<Vec<ProvenanceEntry>>;

    /// Provenance of records across `scope` within `window`, bounded by `limit`.
    async fn evidence_by_scope(
        &self,
        scope: &Scope,
        window: TimeWindow,
        limit: usize,
    ) -> CoreResult<Vec<ProvenanceEntry>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(epoch: i64) -> Timestamp {
        use chrono::TimeZone;
        chrono::Utc
            .timestamp_opt(epoch, 0)
            .single()
            .expect("test ts")
    }

    #[test]
    fn time_window_contains_is_half_open() {
        let w = TimeWindow::open().from(ts(100)).to(ts(200));
        assert!(!w.contains(ts(99)), "below from excluded");
        assert!(w.contains(ts(100)), "from is inclusive");
        assert!(w.contains(ts(199)));
        assert!(!w.contains(ts(200)), "to is exclusive");
        assert!(
            TimeWindow::open().contains(ts(0)),
            "open window matches all"
        );
        assert!(
            TimeWindow::open().from(ts(100)).contains(ts(100)),
            "open upper bound"
        );
    }
}
