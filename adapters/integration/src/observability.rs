//! SQLite implementation of the [`Observability`] port (engram-host-sdk brief, S6).
//!
//! [`SqlObservability`] aggregates diagnostics the provider already holds — the
//! [`CapabilityReport`], the [`EmbeddingProviderConfig`], the schema/adapter
//! versions — and derives [`RecordCounts`] by listing the wired concrete stores.
//! It reuses — never reimplements — the existing capability report and config.
//!
//! Per type, the count is the length of the corresponding store list call,
//! scoped to a fixed diagnostic scope (set when the adapter is constructed; a
//! broad tenant scope in the default wiring). v1 lists knowledge entities /
//! sources / chunks / relationships and beliefs. Types whose store exposes no
//! list API in v1 (`documents`, `memories`) report `0` — degraded, not an error.
//!
//! A store that is unwired (`None`) or whose list call errors reports `0` for
//! that type; the snapshot still returns `Ok`. Diagnostics degrades — never
//! errors — so a host can always read operational state.
//!
//! `slow_query_diagnostics` is `None` in v1 (instrumentation deferred).
//!
//! No schema change: the impl reuses the existing per-store list reads. It is
//! engine-specific (it names `Sql*` and holds the concrete stores), which is why
//! it lives here rather than in the engine-neutral port crate.
//!
//! ADR-0022: only this adapter crate may name `Sql*`; the port it implements
//! stays engine-neutral.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::Scope;
use engram_integration::{
    CapabilityReport, DiagnosticsSnapshot, EmbeddingProviderConfig, Observability, RecordCounts,
};
use engram_runtime::CoreResult;
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;

/// SQLite-backed [`Observability`]: aggregates the provider's existing
/// diagnostics + counts records by listing the wired concrete stores.
///
/// Construct with [`SqlObservability::new`] from the wired concrete knowledge
/// and belief stores (each `None` when that family is unwired), the diagnostic
/// scope, and the provider-level fields (capability report, embedding config,
/// versions) passed through from the provider/configuration. A `None` store or a
/// list error degrades that type's count to `0`; the snapshot still returns `Ok`.
pub struct SqlObservability {
    knowledge: Option<Arc<SqlKnowledgeStore>>,
    beliefs: Option<Arc<SqlBeliefStore>>,
    scope: Scope,
    capabilities: CapabilityReport,
    embedding_config: EmbeddingProviderConfig,
    schema_version: String,
    adapter_version: String,
}

impl SqlObservability {
    /// Wraps the wired concrete stores + provider-level fields to expose one
    /// diagnostic read. Pass `None` for a store whose family is unwired; its
    /// record type will report `0`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        knowledge: Option<Arc<SqlKnowledgeStore>>,
        beliefs: Option<Arc<SqlBeliefStore>>,
        scope: Scope,
        capabilities: CapabilityReport,
        embedding_config: EmbeddingProviderConfig,
        schema_version: impl Into<String>,
        adapter_version: impl Into<String>,
    ) -> Self {
        Self {
            knowledge,
            beliefs,
            scope,
            capabilities,
            embedding_config,
            schema_version: schema_version.into(),
            adapter_version: adapter_version.into(),
        }
    }

    /// Counts knowledge entities visible to the diagnostic scope (degrades to
    /// `0` when the knowledge store is unwired or the list call errors).
    async fn count_entities(&self) -> usize {
        match &self.knowledge {
            Some(store) => store
                .list_entities(&self.scope)
                .await
                .map(|v| v.len())
                .unwrap_or(0),
            None => 0,
        }
    }

    /// Counts knowledge relationships visible to the diagnostic scope.
    async fn count_relationships(&self) -> usize {
        match &self.knowledge {
            Some(store) => store
                .list_relationships(&self.scope)
                .await
                .map(|v| v.len())
                .unwrap_or(0),
            None => 0,
        }
    }

    /// Counts knowledge sources visible to the diagnostic scope.
    async fn count_sources(&self) -> usize {
        match &self.knowledge {
            Some(store) => store
                .list_sources(&self.scope)
                .await
                .map(|v| v.len())
                .unwrap_or(0),
            None => 0,
        }
    }

    /// Counts knowledge chunks visible to the diagnostic scope.
    async fn count_chunks(&self) -> usize {
        match &self.knowledge {
            Some(store) => store
                .list_chunks(&self.scope)
                .await
                .map(|v| v.len())
                .unwrap_or(0),
            None => 0,
        }
    }

    /// Counts beliefs visible to the diagnostic scope (degrades to `0` when the
    /// belief store is unwired or the list call errors).
    async fn count_beliefs(&self) -> usize {
        match &self.beliefs {
            Some(store) => store
                .list_beliefs(&self.scope)
                .await
                .map(|v| v.len())
                .unwrap_or(0),
            None => 0,
        }
    }
}

#[async_trait]
impl Observability for SqlObservability {
    async fn diagnostics(&self) -> CoreResult<DiagnosticsSnapshot> {
        // Counts are derived by listing the wired concrete stores (no new SQL).
        // v1 has no list API for `documents` or `memories` → they stay `0`
        // (honest degraded mode). Each list degrades to `0` on error/None.
        let record_counts = RecordCounts {
            memories: 0,
            entities: self.count_entities().await,
            relationships: self.count_relationships().await,
            sources: self.count_sources().await,
            documents: 0,
            chunks: self.count_chunks().await,
            beliefs: self.count_beliefs().await,
        };

        Ok(DiagnosticsSnapshot {
            capabilities: self.capabilities.clone(),
            record_counts,
            embedding_config: self.embedding_config.clone(),
            schema_version: self.schema_version.clone(),
            adapter_version: self.adapter_version.clone(),
            // v1: slow-query / retrieval diagnostics are deferred (None, not
            // silently absent in the snapshot shape).
            slow_query_diagnostics: None,
        })
    }
}

#[cfg(test)]
mod tests {
    //! The SqlObservability integration tests live in
    //! `adapters/integration/tests/observability.rs` (and the self-sufficient
    //! conformance fixture in `fixtures/observability.rs`) so they can share the
    //! fixture helpers and the block_on driving style. This module is reserved
    //! for any future inline unit tests that do not require a store.
}
