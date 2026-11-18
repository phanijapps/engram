//! SQLite constructor compatibility for the shared observability implementation.
use crate::{
    CapabilityReport, DiagnosticsSnapshot, EmbeddingProviderConfig, Observability,
    observability_composition::SharedObservability, storage_composition::CompositionRead,
};
use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_domain::Scope;
use engram_runtime::CoreResult;
use engram_store_sqlite::{SqlBeliefStore, SqlKnowledgeStore};
use std::sync::Arc;

/// Preserves the concrete constructor while delegating diagnostic behavior.
pub struct SqlObservability(SharedObservability);
impl SqlObservability {
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
        Self(SharedObservability::new(
            knowledge.map(|store| store as Arc<dyn CompositionRead>),
            beliefs.map(|store| store as Arc<dyn BeliefRepository>),
            scope,
            capabilities,
            embedding_config,
            schema_version,
            adapter_version,
        ))
    }
}
#[async_trait]
impl Observability for SqlObservability {
    async fn diagnostics(&self) -> CoreResult<DiagnosticsSnapshot> {
        self.0.diagnostics().await
    }
}
