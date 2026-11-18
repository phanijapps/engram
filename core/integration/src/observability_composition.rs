//! Shared diagnostic assembly over portable storage readers.
//! Optional readers and failed count reads retain the existing zero-count degradation.
//! Capability/configuration/version fields are passed through unchanged.
use std::sync::Arc;

use crate::storage_composition::CompositionRead;
use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_domain::Scope;
use engram_runtime::CoreResult;

use crate::{
    CapabilityReport, DiagnosticsSnapshot, EmbeddingProviderConfig, Observability, RecordCounts,
};

/// Provider-neutral [`Observability`]: aggregates the provider's existing
/// diagnostics + counts records by listing the wired storage contributors.
///
/// Construct with [`SharedObservability::new`] from the wired concrete knowledge
/// and belief stores (each `None` when that family is unwired), the diagnostic
/// scope, and the provider-level fields (capability report, embedding config,
/// versions) passed through from the provider/configuration. A `None` store or a
/// list error degrades that type's count to `0`; the snapshot still returns `Ok`.
pub struct SharedObservability {
    knowledge: Option<Arc<dyn CompositionRead>>,
    beliefs: Option<Arc<dyn BeliefRepository>>,
    scope: Scope,
    capabilities: CapabilityReport,
    embedding_config: EmbeddingProviderConfig,
    schema_version: String,
    adapter_version: String,
}

impl SharedObservability {
    /// Wraps the wired storage contributors + provider-level fields to expose one
    /// diagnostic read. Pass `None` for a store whose family is unwired; its
    /// record type will report `0`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        knowledge: Option<Arc<dyn CompositionRead>>,
        beliefs: Option<Arc<dyn BeliefRepository>>,
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
impl Observability for SharedObservability {
    async fn diagnostics(&self) -> CoreResult<DiagnosticsSnapshot> {
        // Counts are derived by listing the wired storage contributors (existing read ports).
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
