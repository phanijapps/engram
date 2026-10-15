//! Bind diagnostics to the finalized provider report after handle readiness.
use super::{CapabilityReport, DiagnosticsSnapshot, Observability};
use async_trait::async_trait;
use engram_runtime::CoreResult;
use std::sync::Arc;

pub(crate) struct ProviderObservability {
    inner: Arc<dyn Observability>,
    capabilities: CapabilityReport,
}

impl ProviderObservability {
    pub(crate) fn new(inner: Arc<dyn Observability>, capabilities: CapabilityReport) -> Self {
        Self {
            inner,
            capabilities,
        }
    }
}

#[async_trait]
impl Observability for ProviderObservability {
    async fn diagnostics(&self) -> CoreResult<DiagnosticsSnapshot> {
        let mut snapshot = self.inner.diagnostics().await?;
        snapshot.capabilities = self.capabilities.clone();
        Ok(snapshot)
    }
}
