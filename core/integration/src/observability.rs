//! Backend-neutral observability port (engram-host-sdk brief, S6).
//!
//! [`Observability`] exposes one diagnostic handle that aggregates the
//! operational state a provider already holds — the [`CapabilityReport`], record
//! counts by semantic type, the embedding configuration, schema/adapter
//! versions — into a single [`DiagnosticsSnapshot`]. A host reads this to answer
//! "what is this provider, and how much data does it hold?" through one
//! backend-neutral call.
//!
//! v1 limitations (honest degraded mode, not silent absence):
//! - **Record counts** are derived by listing the wired concrete stores (no new
//!   SQL, no schema change). Types whose store exposes no list API
//!   (`documents`, `memories` in v1) report `0` — degraded, not an error. Counts
//!   are scope-visible counts: the diagnostic scope is fixed when the adapter is
//!   constructed (a broad tenant scope in the default wiring).
//! - **Slow-query / retrieval diagnostics** are `None` in v1; real
//!   instrumentation is deferred (requires adapter-level timing).
//!
//! ADR-0022: this port is engine-neutral — it names no engine type and holds no
//! SQL (enforced by `.codex/hooks/check-engine-neutrality.sh`). The SQLite
//! implementation lives in the adapters layer (`engram-conformance`).

use async_trait::async_trait;
use engram_runtime::CoreResult;
use serde::{Deserialize, Serialize};

use crate::capability::CapabilityReport;
use crate::config::EmbeddingProviderConfig;

/// Record counts by semantic type, derived by listing the wired concrete stores.
///
/// Each field is `0` when the underlying store is unavailable or exposes no list
/// API for that type (degraded, not an error). v1 derives counts for knowledge
/// entities / sources / chunks / relationships and beliefs; `documents` and
/// `memories` are `0` until the corresponding stores expose a list API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordCounts {
    /// Memory records (v1: no list API → `0`).
    pub memories: usize,
    /// Knowledge graph entities.
    pub entities: usize,
    /// Knowledge graph relationships (edges).
    pub relationships: usize,
    /// Registered knowledge sources.
    pub sources: usize,
    /// Source documents (v1: no list API → `0`).
    pub documents: usize,
    /// Document chunks (the smallest retrievable source-grounded unit).
    pub chunks: usize,
    /// Beliefs.
    pub beliefs: usize,
}

impl RecordCounts {
    /// Creates an all-zero count snapshot (every store unavailable / degraded).
    pub fn empty() -> Self {
        Self {
            memories: 0,
            entities: 0,
            relationships: 0,
            sources: 0,
            documents: 0,
            chunks: 0,
            beliefs: 0,
        }
    }
}

impl Default for RecordCounts {
    fn default() -> Self {
        Self::empty()
    }
}

/// A point-in-time diagnostic snapshot of one provider.
///
/// Aggregates fields the provider already holds — never recomputes them. The
/// [`CapabilityReport`] is delegated (not re-derived); the
/// [`EmbeddingProviderConfig`] and versions are passed through from the provider
/// configuration. [`RecordCounts`] are derived by listing the wired concrete
/// stores (no new SQL). `slow_query_diagnostics` is `None` in v1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    /// The provider's capability report (18 keys), delegated as-is.
    pub capabilities: CapabilityReport,
    /// Record counts by semantic type, derived by listing the wired stores.
    pub record_counts: RecordCounts,
    /// The embedding provider configuration (provider/model/dimensions/...).
    pub embedding_config: EmbeddingProviderConfig,
    /// The storage schema version the provider reports.
    pub schema_version: String,
    /// The adapter version the provider reports.
    pub adapter_version: String,
    /// Slow-query / retrieval diagnostics. `None` in v1 (instrumentation
    /// deferred); never silently absent in the snapshot shape.
    pub slow_query_diagnostics: Option<String>,
}

/// Observability port: one backend-neutral diagnostic read of a provider.
///
/// `diagnostics` aggregates the existing capability report, embedding config,
/// versions, and record counts (via listing) into one [`DiagnosticsSnapshot`].
/// It degrades — never errors — when a store is unavailable: that type's count
/// is reported as `0` and the snapshot still returns `Ok`.
#[async_trait]
pub trait Observability: Send + Sync {
    /// Returns a point-in-time [`DiagnosticsSnapshot`] of this provider.
    async fn diagnostics(&self) -> CoreResult<DiagnosticsSnapshot>;
}
